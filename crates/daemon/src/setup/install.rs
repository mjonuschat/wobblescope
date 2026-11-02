//! Install, check and remove, in the order that leaves the most recoverable state if a step
//! fails: ask Moonraker first, then change files, then restart what needs restarting.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use super::host::{Host, write_atomic};
use super::moonraker::{Discovery, Guard, Moonraker, MoonrakerError, Verification};
use super::render::{self, SERVICE};
use super::{Options, SetupError};
use crate::config::DEFAULT_MOONRAKER_URL;

/// Left in the install folder before a change Moonraker only sees after a restart, and deleted
/// once that restart happened.
const MOONRAKER_MARKER: &str = ".pending-moonraker-restart";
/// The same for a change the service only sees after a restart.
const SERVICE_MARKER: &str = ".pending-service-restart";
/// The file Moonraker's updater reads.
const RELEASE_FILE: &str = "release_info.json";

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
    release: PathBuf,
    config: PathBuf,
    unit: PathBuf,
    conf: PathBuf,
    asvc: Result<PathBuf, String>,
}

impl Paths {
    fn new(host: &Host, disc: &Discovery, asset: &str) -> Self {
        let install_dir = host.invoker.home.join(SERVICE);
        Self {
            binary: install_dir.join(asset),
            release: install_dir.join(RELEASE_FILE),
            config: disc.config_root.join("wobblescope.toml"),
            unit: host.unit_dir.join(format!("{SERVICE}.service")),
            conf: disc.main_config.clone(),
            asvc: disc.data_path.clone().map(|p| p.join("moonraker.asvc")),
            install_dir,
        }
    }

    fn marker(&self, name: &str) -> PathBuf {
        self.install_dir.join(name)
    }
}

/// Prints one line per step as it happens, so a run that stops still shows what was done.
struct Say<'a> {
    out: &'a mut dyn Write,
}

impl Say<'_> {
    fn step(&mut self, label: &str, state: &str) {
        let _ = writeln!(self.out, "  {label:<26} {state}");
    }

    fn note(&mut self, text: &str) {
        let _ = writeln!(self.out, "    {text}");
    }
}

fn io_err(what: impl std::fmt::Display) -> impl FnOnce(io::Error) -> SetupError {
    move |e| SetupError::new(format!("{what}: {e}"))
}

fn moonraker_err(e: MoonrakerError) -> SetupError {
    SetupError::new(e.to_string())
}

fn read(path: &Path) -> Result<Option<String>, SetupError> {
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

/// Creates the empty marker `path` when it is not there yet.
fn mark(path: &Path, owner: Option<(u32, u32)>) -> Result<(), SetupError> {
    if !path.exists() {
        write_atomic(path, b"", Some(0o644), owner)
            .map_err(io_err(format!("cannot write {}", path.display())))?;
    }
    Ok(())
}

fn clear(path: &Path) {
    let _ = fs::remove_file(path);
}

/// Rewrites a Moonraker-owned file, keeping its owner and mode, after the marker is down.
fn edit_moonraker_file(
    path: &Path,
    text: &str,
    marker: &Path,
    owner: Option<(u32, u32)>,
) -> Result<(), SetupError> {
    mark(marker, owner)?;
    write_atomic(path, text.as_bytes(), None, None)
        .map_err(io_err(format!("cannot update {}", path.display())))
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) if x == y => return true,
        _ => {}
    }
    matches!((fs::read(a), fs::read(b)), (Ok(x), Ok(y)) if x == y)
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

/// Installs or repairs everything.
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
    let asset = render::asset_name()
        .ok_or_else(|| SetupError::new("no release asset is published for this architecture"))?;
    let disc = discover(host, mr).await?;
    let paths = Paths::new(host, &disc, asset);
    let owner = host.owner();
    let mut say = Say { out };
    let mut ok = true;
    // Read before anything changes, so a main config file that cannot be read does not leave a
    // partial install.
    let conf = read(&paths.conf)?.ok_or_else(|| {
        SetupError::new(format!(
            "{} does not exist, though Moonraker reports it as its main config file. \
             Nothing was changed.",
            paths.conf.display()
        ))
    })?;
    warn_logins(&mut say, &disc);

    if !paths.install_dir.exists() {
        fs::create_dir_all(&paths.install_dir).map_err(io_err(format!(
            "cannot create {}",
            paths.install_dir.display()
        )))?;
        if let Some((uid, gid)) = owner {
            std::os::unix::fs::chown(&paths.install_dir, Some(uid), Some(gid)).map_err(io_err(
                format!("cannot chown {}", paths.install_dir.display()),
            ))?;
        }
    }
    let service_marker = paths.marker(SERVICE_MARKER);
    let moonraker_marker = paths.marker(MOONRAKER_MARKER);
    let mut service_changed = false;
    let mut moonraker_changed = false;

    // The unit, then the files it points at.
    let unit_text = render::unit(&host.invoker.name, &paths.binary, &paths.config);
    if read(&paths.unit)?.as_deref() != Some(unit_text.as_str()) {
        mark(&service_marker, owner)?;
    }
    let step = put(&paths.unit, &unit_text, 0o644, None)?;
    if step.changed() {
        service_changed = true;
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

    let same = paths.binary.exists() && same_bytes(&host.binary, &paths.binary);
    let step = if same {
        Step::Current
    } else {
        mark(&service_marker, owner)?;
        let bytes = fs::read(&host.binary)
            .map_err(io_err(format!("cannot read {}", host.binary.display())))?;
        let existed = paths.binary.exists();
        write_atomic(&paths.binary, &bytes, Some(0o755), owner)
            .map_err(io_err(format!("cannot write {}", paths.binary.display())))?;
        if existed {
            Step::Repaired
        } else {
            Step::Installed
        }
    };
    service_changed |= step.changed();
    say.step("binary", step.text());

    let release = render::release_info(asset, env!("CARGO_PKG_VERSION"));
    say.step(
        "release_info.json",
        put(&paths.release, &release, 0o644, owner)?.text(),
    );

    match read(&paths.config)? {
        None => {
            mark(&service_marker, owner)?;
            let (text, mode) = render::config_file(wanted_url(opts), opts.api_key.as_deref());
            put(&paths.config, &text, mode, owner)?;
            service_changed = true;
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

    // Moonraker's own files.
    match &paths.asvc {
        Ok(asvc) => {
            let text = read(asvc)?.unwrap_or_default();
            match render::with_service(&text, SERVICE) {
                Some(updated) => {
                    edit_moonraker_file(asvc, &updated, &moonraker_marker, owner)?;
                    moonraker_changed = true;
                    say.step("moonraker.asvc", "installed");
                }
                None => say.step("moonraker.asvc", "already current"),
            }
        }
        Err(note) => say.step("moonraker.asvc", &format!("skipped: {note}")),
    }
    let block = render::update_block(&paths.install_dir);
    if render::has_block(&conf) {
        match render::with_block(&conf, &block) {
            Some(updated) => {
                edit_moonraker_file(&paths.conf, &updated, &moonraker_marker, owner)?;
                moonraker_changed = true;
                say.step("update_manager section", "repaired");
            }
            None => say.step("update_manager section", "already current"),
        }
    } else if disc.wobblescope_loaded() || render::has_own_section(&conf) {
        say.step("update_manager section", "skipped: you already have one");
        let here = paths.install_dir.to_string_lossy();
        if disc
            .wobblescope_path
            .as_deref()
            .is_some_and(|p| p.trim_end_matches('/') != here)
        {
            say.note(&format!(
                "warning: your [update_manager wobblescope] path is not {here}, so it would \
                 update a different binary from the one the service runs"
            ));
        }
    } else {
        if let Some(updated) = render::with_block(&conf, &block) {
            edit_moonraker_file(&paths.conf, &updated, &moonraker_marker, owner)?;
        }
        moonraker_changed = true;
        say.step("update_manager section", "installed");
    }

    // Restart the service.
    let active = host.systemctl(&["is-active", SERVICE])?;
    let active = active.success && active.stdout == "active";
    if service_changed || service_marker.exists() || !active {
        mark(&service_marker, owner)?;
        host.systemctl_ok(&["restart", SERVICE])?;
        if stays_active(host).await? {
            clear(&service_marker);
            say.step("service", "restarted and running");
        } else {
            ok = false;
            say.step("service", "FAILED to stay running");
            say.note(&format!(
                "see: journalctl -u {SERVICE} -n 50. Likely causes: the listen port 7126 is \
                 already in use, or {} is not valid.",
                paths.config.display()
            ));
        }
    } else {
        say.step("service", "already current");
    }

    // Restart Moonraker.
    let needs = moonraker_changed
        || moonraker_marker.exists()
        || (disc.has_update_manager() && !disc.wobblescope_loaded());
    if !needs {
        say.step("moonraker", "already current");
        if disc.has_update_manager() {
            // A verdict that failed on an earlier run must not turn into success on a rerun.
            match mr.entry().await.map_err(moonraker_err)? {
                Some(verdict) if verdict.valid => {
                    say.step("update manager", "lists wobblescope as valid");
                }
                Some(verdict) => {
                    ok = false;
                    say.step("update manager", "lists wobblescope as NOT valid");
                    for line in verdict.warnings.iter().chain(&verdict.anomalies) {
                        say.note(&format!("Moonraker says: {line}"));
                    }
                }
                None => {
                    ok = false;
                    say.step("update manager", "does not list wobblescope");
                }
            }
        }
        return Ok(ok);
    }
    mark(&moonraker_marker, owner)?;
    match mr.print_guard().await.map_err(moonraker_err)? {
        Guard::Blocked(why) => {
            say.step("moonraker restart", &format!("NOT DONE: {why}"));
            say.note("everything else is done; run `sudo wobblescope setup` again once it is safe to restart Moonraker");
            return Ok(false);
        }
        Guard::Clear => {}
    }
    mr.restart().await.map_err(moonraker_err)?;
    clear(&moonraker_marker);
    say.step("moonraker restart", "done");
    if disc.has_update_manager() {
        match mr.verify().await {
            Verification::Listed(verdict) if verdict.valid => {
                say.step("update manager", "lists wobblescope as valid");
            }
            Verification::Listed(verdict) => {
                ok = false;
                say.step("update manager", "lists wobblescope as NOT valid");
                for line in verdict.warnings.iter().chain(&verdict.anomalies) {
                    say.note(&format!("Moonraker says: {line}"));
                }
            }
            Verification::TimedOut => {
                ok = false;
                say.step("update manager", "did not list wobblescope in time");
            }
        }
    } else {
        say.step(
            "update manager",
            "skipped: Moonraker has no [update_manager] section",
        );
        say.note("add an [update_manager] section to moonraker.conf for updates; setup does not");
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
    let asset = render::asset_name()
        .ok_or_else(|| SetupError::new("no release asset is published for this architecture"))?;
    let disc = discover(host, mr).await?;
    let paths = Paths::new(host, &disc, asset);
    let mut say = Say { out };
    let mut ok = true;
    let mut report = |say: &mut Say, label: &str, good: bool, state: &str| {
        ok &= good;
        say.step(label, state);
    };
    warn_logins(&mut say, &disc);

    let binary = if !paths.binary.exists() {
        (false, "missing")
    } else if same_bytes(&host.binary, &paths.binary) {
        (true, "ok")
    } else {
        (false, "stale: differs from this binary")
    };
    report(&mut say, "binary", binary.0, binary.1);
    let release = render::release_info(asset, env!("CARGO_PKG_VERSION"));
    let state = match read(&paths.release)? {
        None => (false, "missing"),
        Some(t) if t == release => (true, "ok"),
        Some(_) => (false, "stale"),
    };
    report(&mut say, "release_info.json", state.0, state.1);
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
    let conf = read(&paths.conf)?.unwrap_or_default();
    let block = render::update_block(&paths.install_dir);
    let state = if render::has_block(&conf) {
        if render::with_block(&conf, &block).is_none() {
            (true, "ok")
        } else {
            (false, "stale")
        }
    } else if render::has_own_section(&conf) || disc.wobblescope_loaded() {
        (true, "ok (yours)")
    } else {
        (false, "missing")
    };
    report(&mut say, "update_manager section", state.0, state.1);

    if disc.has_update_manager() {
        match mr.entry().await.map_err(moonraker_err)? {
            Some(v) if v.valid => report(&mut say, "moonraker verdict", true, "valid"),
            Some(v) => {
                report(&mut say, "moonraker verdict", false, "NOT valid");
                for line in v.warnings.iter().chain(&v.anomalies) {
                    say.note(&format!("Moonraker says: {line}"));
                }
            }
            None => report(&mut say, "moonraker verdict", false, "not listed"),
        }
    } else {
        say.step("moonraker verdict", "skipped: no [update_manager] section");
    }
    for (name, what) in [
        (MOONRAKER_MARKER, "Moonraker"),
        (SERVICE_MARKER, "the service"),
    ] {
        if paths.marker(name).exists() {
            report(&mut say, "pending restart", false, what);
        }
    }
    Ok(ok)
}

/// Deletes the binary and the release file. Removal does this last, so a run deferred by a
/// print can be finished with the very binary it deferred.
fn remove_install_files(paths: &Paths, say: &mut Say) -> Result<(), SetupError> {
    for file in [&paths.binary, &paths.release] {
        if file.exists() {
            fs::remove_file(file).map_err(io_err(format!("cannot remove {}", file.display())))?;
        }
    }
    say.step("install folder", "binary and release file removed");
    Ok(())
}

/// Takes the install apart. The config file and the recorded sessions stay.
///
/// # Errors
/// [`SetupError`] for a failure that stops the run.
pub async fn remove(
    host: &Host,
    mr: &Moonraker,
    _opts: &Options,
    out: &mut dyn Write,
) -> Result<bool, SetupError> {
    let asset = render::asset_name()
        .ok_or_else(|| SetupError::new("no release asset is published for this architecture"))?;
    let disc = discover(host, mr).await?;
    let paths = Paths::new(host, &disc, asset);
    let owner = host.owner();
    let moonraker_marker = paths.marker(MOONRAKER_MARKER);
    let mut say = Say { out };
    let mut ok = true;
    let mut changed = false;

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
                edit_moonraker_file(asvc, &updated, &moonraker_marker, owner)?;
                changed = true;
                say.step("moonraker.asvc", "entry removed");
            }
            None => say.step("moonraker.asvc", "no entry"),
        },
        Err(note) => say.step("moonraker.asvc", &format!("skipped: {note}")),
    }
    let conf = read(&paths.conf)?.unwrap_or_default();
    match render::without_block(&conf) {
        Some(updated) => {
            edit_moonraker_file(&paths.conf, &updated, &moonraker_marker, owner)?;
            changed = true;
            say.step("update_manager section", "removed");
        }
        None if render::has_own_section(&conf) => {
            say.step("update_manager section", "left alone: you wrote it");
        }
        None => say.step("update_manager section", "none to remove"),
    }

    let own_section_loaded = disc.wobblescope_loaded() && !render::has_own_section(&conf);
    if changed || moonraker_marker.exists() || own_section_loaded {
        if paths.install_dir.exists() {
            mark(&moonraker_marker, owner)?;
        }
        match mr.print_guard().await.map_err(moonraker_err)? {
            Guard::Blocked(why) => {
                say.step("moonraker restart", &format!("NOT DONE: {why}"));
                say.step("install folder", "kept until Moonraker is restarted");
                say.note("run `sudo wobblescope setup --remove` again once it is safe to restart Moonraker");
                return Ok(false);
            }
            Guard::Clear => {}
        }
        mr.restart().await.map_err(moonraker_err)?;
        clear(&moonraker_marker);
        say.step("moonraker restart", "done");
        remove_install_files(&paths, &mut say)?;
        if disc.has_update_manager() {
            let after = discover(host, mr).await?;
            if after.wobblescope_loaded() {
                say.step(
                    "update manager",
                    "still has a wobblescope section you wrote",
                );
            } else if mr.entry().await.map_err(moonraker_err)?.is_some() {
                ok = false;
                say.step("update manager", "still lists wobblescope");
            } else {
                say.step("update manager", "no longer lists wobblescope");
            }
        }
    } else {
        say.step("moonraker", "already current");
        remove_install_files(&paths, &mut say)?;
    }
    if fs::read_dir(&paths.install_dir).is_ok_and(|mut d| d.next().is_none()) {
        let _ = fs::remove_dir(&paths.install_dir);
    }
    say.note("kept: wobblescope.toml and the recorded sessions");
    Ok(ok)
}
