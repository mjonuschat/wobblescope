//! Install, check and remove, in the order that leaves the most recoverable state if a step
//! fails: ask Moonraker first, then change files, then restart the service. The binary, its
//! `release_info.json`, the `update_manager` section and the Moonraker restart belong to the
//! installer script, not to setup.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::host::{Host, write_atomic};
use super::moonraker::{Discovery, Moonraker, MoonrakerError};
use super::render::{self, SERVICE};
use super::{Options, SetupError, nginx_link};
use crate::config::{self, DEFAULT_MOONRAKER_URL, Settings};

/// What a step found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Installed,
    Repaired,
    Current,
}

impl Step {
    fn text(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Repaired => "repaired",
            Self::Current => "already current",
        }
    }

    fn changed(self) -> bool {
        self != Self::Current
    }
}

/// Where everything lives.
struct Paths {
    install_dir: PathBuf,
    binary: PathBuf,
    config: PathBuf,
    unit: PathBuf,
    asvc: Result<PathBuf, String>,
}

impl Paths {
    fn new(host: &Host, disc: &Discovery) -> Self {
        let install_dir = host
            .binary
            .parent()
            .map_or_else(|| host.invoker.home.clone(), Path::to_path_buf);
        Self {
            binary: host.binary.clone(),
            config: disc.config_root.join("wobblescope.toml"),
            unit: host.unit_dir.join(format!("{SERVICE}.service")),
            asvc: disc.data_path.clone().map(|p| p.join("moonraker.asvc")),
            install_dir,
        }
    }
}

/// Prints one line per step as it happens, so a run that stops still shows what was done.
pub(super) struct Say<'a> {
    pub(super) out: &'a mut dyn Write,
}

impl Say<'_> {
    pub(super) fn step(&mut self, label: &str, state: &str) {
        let _ = writeln!(self.out, "  {label:<26} {state}");
    }

    pub(super) fn note(&mut self, text: &str) {
        let _ = writeln!(self.out, "    {text}");
    }
}

pub(super) fn io_err(what: impl std::fmt::Display) -> impl FnOnce(io::Error) -> SetupError {
    move |e| SetupError::new(format!("{what}: {e}"))
}

fn moonraker_err(e: MoonrakerError) -> SetupError {
    SetupError::new(e.to_string())
}

pub(super) fn read(path: &Path) -> Result<Option<String>, SetupError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(SetupError::new(format!(
            "cannot read {}: {e}",
            path.display()
        ))),
    }
}

/// Writes `text` to `path` unless it already holds exactly that.
fn put(path: &Path, text: &str, mode: u32, owner: Option<(u32, u32)>) -> Result<Step, SetupError> {
    let existing = read(path)?;
    if existing.as_deref() == Some(text) {
        return Ok(Step::Current);
    }
    write_atomic(path, text.as_bytes(), Some(mode), owner)
        .map_err(io_err(format!("cannot write {}", path.display())))?;
    Ok(if existing.is_some() {
        Step::Repaired
    } else {
        Step::Installed
    })
}

/// Rewrites a Moonraker-owned file, keeping its owner and mode.
fn edit_moonraker_file(path: &Path, text: &str) -> Result<(), SetupError> {
    write_atomic(path, text.as_bytes(), None, None)
        .map_err(io_err(format!("cannot update {}", path.display())))
}

fn wanted_url(opts: &Options) -> Option<&str> {
    opts.moonraker
        .as_deref()
        .filter(|u| u.trim_end_matches('/') != DEFAULT_MOONRAKER_URL)
}

async fn discover(host: &Host, mr: &Moonraker) -> Result<Discovery, SetupError> {
    mr.discover(host.invoker.uid).await.map_err(moonraker_err)
}

fn warn_logins(say: &mut Say, disc: &Discovery) {
    if disc.login_required {
        say.note(
            "warning: Moonraker requires logins (force_logins), so the daemon refuses every \
             request. Setup does not edit that; turn it off in moonraker.conf or log in.",
        );
    }
}

/// Waits `settle`, then reports whether the service is active.
async fn stays_active(host: &Host) -> Result<bool, SetupError> {
    tokio::time::sleep(host.settle).await;
    let outcome = host.systemctl(&["is-active", SERVICE])?;
    Ok(outcome.success && outcome.stdout == "active")
}

/// The port the daemon's Moonraker URL names.
fn moonraker_port(settings: &Settings) -> u16 {
    let uri = &settings.moonraker_url;
    uri.port_u16()
        .unwrap_or(if uri.scheme_str() == Some("https") {
            443
        } else {
            80
        })
}

/// Publishes the daemon through nginx. The daemon must trust nginx's loopback connections
/// before nginx forwards anything to it, so `trusted_proxies` is written first and a failure
/// there stops the step before nginx is touched.
fn nginx_step(host: &Host, say: &mut Say, config: &Path) -> Result<bool, SetupError> {
    let settings = match config::load(Some(config)) {
        Ok(settings) => settings,
        Err(e) => {
            say.step("nginx", "skipped: wobblescope.toml is not valid");
            say.note(&e.to_string());
            return Ok(true);
        }
    };
    let Some(plan) = nginx_link::find(host, say, moonraker_port(&settings)) else {
        return Ok(true);
    };
    let text = read(config)?.unwrap_or_default();
    match render::with_trusted_proxies(&text) {
        Ok(Some(updated)) => {
            edit_moonraker_file(config, &updated)?;
            say.step("trusted_proxies", "added 127.0.0.1 and ::1");
        }
        Ok(None) => say.step("trusted_proxies", "already current"),
        Err(why) => {
            say.step("trusted_proxies", "FAILED, nginx left alone");
            say.note(&format!("{}: {why}", config.display()));
            return Ok(false);
        }
    }
    nginx_link::install(host, say, &plan, settings.listen.port())
}

/// Installs or repairs the unit, the config file and the service-list entry, then restarts the
/// service.
///
/// # Errors
/// [`SetupError`] for a failure that stops the run; steps already done stay done and were
/// printed.
pub async fn install(
    host: &Host,
    mr: &Moonraker,
    opts: &Options,
    out: &mut dyn Write,
) -> Result<bool, SetupError> {
    let disc = discover(host, mr).await?;
    let paths = Paths::new(host, &disc);
    let owner = host.owner();
    let mut say = Say { out };
    let mut ok = true;
    warn_logins(&mut say, &disc);

    // The unit, then the file it points at.
    let unit_text = render::unit(&host.invoker.name, &paths.binary, &paths.config);
    let step = put(&paths.unit, &unit_text, 0o644, None)?;
    if step.changed() {
        host.systemctl_ok(&["daemon-reload"])?;
    }
    say.step("service unit", step.text());
    let enabled = host.systemctl(&["is-enabled", SERVICE])?;
    if enabled.success {
        say.step("service enabled", "already current");
    } else {
        host.systemctl_ok(&["enable", SERVICE])?;
        say.step("service enabled", "installed");
    }

    match read(&paths.config)? {
        None => {
            let (text, mode) = render::config_file(wanted_url(opts), opts.api_key.as_deref());
            put(&paths.config, &text, mode, owner)?;
            say.step("wobblescope.toml", "installed");
        }
        Some(existing) => {
            say.step("wobblescope.toml", "already current (left alone)");
            if opts.api_key.is_some() {
                say.note("--api-key not applied: the config file already exists");
            }
            if let Some(url) = &opts.moonraker {
                let current = existing
                    .parse::<toml::Table>()
                    .ok()
                    .and_then(|t| t.get("moonraker_url")?.as_str().map(str::to_owned))
                    .unwrap_or_else(|| DEFAULT_MOONRAKER_URL.to_owned());
                if current.trim_end_matches('/') != url.trim_end_matches('/') {
                    say.note("--moonraker not applied: the config file already exists");
                }
            }
        }
    }

    if opts.nginx {
        ok &= nginx_step(host, &mut say, &paths.config)?;
    } else {
        say.step("nginx", "skipped (--no-nginx)");
    }

    match &paths.asvc {
        Ok(asvc) => {
            let text = read(asvc)?.unwrap_or_default();
            match render::with_service(&text, SERVICE) {
                Some(updated) => {
                    edit_moonraker_file(asvc, &updated)?;
                    say.step("moonraker.asvc", "installed");
                    say.note("restart Moonraker for wobblescope to appear in the service menu");
                }
                None => say.step("moonraker.asvc", "already current"),
            }
        }
        Err(note) => say.step("moonraker.asvc", &format!("skipped: {note}")),
    }

    // Restart rather than start: the installer may have replaced the binary under a running
    // service, and only a restart runs the new one.
    host.systemctl_ok(&["restart", SERVICE])?;
    if stays_active(host).await? {
        say.step("service", "restarted and running");
    } else {
        ok = false;
        say.step("service", "FAILED to stay running");
        say.note(&format!(
            "see: journalctl -u {SERVICE} -n 50. Likely causes: the listen port 7140 is \
             already in use, or {} is not valid.",
            paths.config.display()
        ));
    }
    Ok(ok)
}

/// Reports the state of each piece and changes nothing.
///
/// # Errors
/// [`SetupError`] when Moonraker cannot be asked.
pub async fn check(
    host: &Host,
    mr: &Moonraker,
    _opts: &Options,
    out: &mut dyn Write,
) -> Result<bool, SetupError> {
    let disc = discover(host, mr).await?;
    let paths = Paths::new(host, &disc);
    let mut say = Say { out };
    let mut ok = true;
    let mut report = |say: &mut Say, label: &str, good: bool, state: &str| {
        ok &= good;
        say.step(label, state);
    };
    warn_logins(&mut say, &disc);

    let exists = paths.config.exists();
    report(
        &mut say,
        "wobblescope.toml",
        exists,
        if exists { "ok" } else { "missing" },
    );

    let unit_text = render::unit(&host.invoker.name, &paths.binary, &paths.config);
    let state = match read(&paths.unit)? {
        None => (false, "missing"),
        Some(t) if t == unit_text => (true, "ok"),
        Some(_) => (false, "stale"),
    };
    report(&mut say, "service unit", state.0, state.1);
    let enabled = host.systemctl(&["is-enabled", SERVICE])?.success;
    report(
        &mut say,
        "service enabled",
        enabled,
        if enabled { "ok" } else { "no" },
    );
    let active = host.systemctl(&["is-active", SERVICE])?;
    let active = active.success && active.stdout == "active";
    report(
        &mut say,
        "service active",
        active,
        if active { "ok" } else { "no" },
    );

    match &paths.asvc {
        Ok(asvc) => {
            let listed = read(asvc)?.is_some_and(|t| render::with_service(&t, SERVICE).is_none());
            report(
                &mut say,
                "moonraker.asvc",
                listed,
                if listed { "ok" } else { "missing" },
            );
        }
        Err(note) => say.step("moonraker.asvc", &format!("skipped: {note}")),
    }
    Ok(ok)
}

/// Stops and removes the service and its service-list entry. The binary, `release_info.json`,
/// the `update_manager` section, the config file and the recorded sessions stay.
///
/// # Errors
/// [`SetupError`] for a failure that stops the run.
pub async fn remove(
    host: &Host,
    mr: &Moonraker,
    _opts: &Options,
    out: &mut dyn Write,
) -> Result<bool, SetupError> {
    let disc = discover(host, mr).await?;
    let paths = Paths::new(host, &disc);
    let mut say = Say { out };

    if paths.unit.exists() {
        host.systemctl_ok(&["disable", "--now", SERVICE])?;
        fs::remove_file(&paths.unit)
            .map_err(io_err(format!("cannot remove {}", paths.unit.display())))?;
        host.systemctl_ok(&["daemon-reload"])?;
        say.step("service", "stopped, disabled and removed");
    } else {
        say.step("service", "not installed");
    }

    match &paths.asvc {
        Ok(asvc) => match read(asvc)?.and_then(|t| render::without_service(&t, SERVICE)) {
            Some(updated) => {
                edit_moonraker_file(asvc, &updated)?;
                say.step("moonraker.asvc", "entry removed");
                say.note("restart Moonraker to drop wobblescope from the service menu");
            }
            None => say.step("moonraker.asvc", "no entry"),
        },
        Err(note) => say.step("moonraker.asvc", &format!("skipped: {note}")),
    }
    say.note(&format!(
        "kept: wobblescope.toml, the recorded sessions, and {}, with its release_info.json and \
         the [update_manager wobblescope] section in moonraker.conf",
        paths.install_dir.display()
    ));
    Ok(true)
}
