//! The nginx step of setup. It writes a snippet that serves the daemon under `/wobblescope/`, and one
//! marked include line in each site that proxies this machine's Moonraker.
//!
//! The snippet is a root-owned regular file, never a symlink, because a root nginx must not
//! include a file a user can write. Site files are edited in place (through symlinks) after
//! one backup, and put back if `nginx -t` rejects the result.

use std::fs;
use std::path::{Path, PathBuf};

use super::SetupError;
use super::host::{Host, write_atomic};
use super::install::{Say, io_err, read};
use super::nginx::{BACKUP_SUFFIX, Layout, Plan, Server};
use super::render;

const SNIPPET: &str = "snippets/wobblescope.conf";

/// Where nginx is looked for. Debian-style systems keep it in `/usr/sbin`, which an ordinary
/// user's `PATH` does not include.
const NGINX_PROGRAMS: [&str; 3] = ["nginx", "/usr/sbin/nginx", "/sbin/nginx"];

/// The first of [`NGINX_PROGRAMS`] that runs.
fn nginx_program(host: &Host) -> Option<&'static str> {
    NGINX_PROGRAMS
        .into_iter()
        .find(|program| host.runner.run(program, &["-v"]).is_ok())
}

/// Prints the one-line reasons nothing can be done, and returns the plan otherwise.
/// Not finding nginx or a site is not an error.
pub(super) fn find(host: &Host, say: &mut Say, moonraker_port: u16) -> Option<Plan> {
    if nginx_program(host).is_none() {
        say.step("nginx", "not found on PATH, /usr/sbin or /sbin, skipped");
        return None;
    }
    let Some(layout) = Layout::read(&host.nginx_dir) else {
        say.step(
            "nginx",
            &format!("no {}/nginx.conf, skipped", host.nginx_dir.display()),
        );
        return None;
    };
    for (path, why) in &layout.skipped {
        say.note(&format!("nginx: skipped {}: {why}", path.display()));
    }
    let plan = layout.plan(moonraker_port);
    for (server, why) in &plan.left_alone {
        say.note(&format!("nginx: left {} alone: {why}", server.label()));
    }
    if plan.mapped.is_empty() {
        say.step(
            "nginx",
            &format!("no site proxies Moonraker on port {moonraker_port}, skipped"),
        );
        return None;
    }
    Some(plan)
}

#[must_use]
pub(super) fn snippet_path(host: &Host) -> PathBuf {
    host.nginx_dir.join(SNIPPET)
}

pub(super) fn target_of(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SnippetState {
    Missing,
    Current,
    /// There, but differs, is a symlink, or has the wrong mode or owner.
    Stale,
}

pub fn snippet_state(host: &Host, path: &Path, wanted: &str) -> Result<SnippetState, SetupError> {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = fs::symlink_metadata(path) else {
        return Ok(SnippetState::Missing);
    };
    let root_owned = !host.chown || (meta.uid() == 0 && meta.gid() == 0);
    let regular = meta.file_type().is_file() && meta.mode() & 0o7777 == 0o644 && root_owned;
    Ok(if regular && read(path)?.as_deref() == Some(wanted) {
        SnippetState::Current
    } else {
        SnippetState::Stale
    })
}

fn write_snippet(host: &Host, path: &Path, text: &str) -> Result<(), SetupError> {
    let dir = path.parent().unwrap_or(Path::new("/"));
    fs::create_dir_all(dir).map_err(io_err(format!("cannot create {}", dir.display())))?;
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        fs::remove_file(path).map_err(io_err(format!("cannot replace {}", path.display())))?;
    }
    write_atomic(
        path,
        text.as_bytes(),
        Some(0o644),
        host.chown.then_some((0, 0)),
    )
    .map_err(io_err(format!("cannot write {}", path.display())))
}

/// The files with sites in `servers`, each once, with the sites in descending offset order so
/// an edit does not move the offsets of the ones after it.
fn by_file(servers: &[Server]) -> Vec<(PathBuf, Vec<&Server>)> {
    let mut files: Vec<(PathBuf, Vec<&Server>)> = Vec::new();
    for server in servers {
        let target = target_of(&server.file);
        match files.iter_mut().find(|(t, _)| *t == target) {
            Some((_, group)) => group.push(server),
            None => files.push((target, vec![server])),
        }
    }
    for (_, group) in &mut files {
        group.sort_by_key(|s| std::cmp::Reverse(s.open));
    }
    files
}

/// Keeps `text`, the file's content before setup first changes it, beside the file.
/// Returns the backup's path when this call created it.
pub(super) fn back_up(target: &Path, text: &str) -> Result<Option<PathBuf>, SetupError> {
    use std::os::unix::fs::MetadataExt;
    let mut name = target.as_os_str().to_owned();
    name.push(BACKUP_SUFFIX);
    let backup = PathBuf::from(name);
    if backup.exists() {
        return Ok(None);
    }
    let meta = fs::metadata(target).map_err(io_err(format!("cannot read {}", target.display())))?;
    write_atomic(
        &backup,
        text.as_bytes(),
        Some(meta.mode() & 0o7777),
        Some((meta.uid(), meta.gid())),
    )
    .map_err(io_err(format!("cannot write {}", backup.display())))?;
    Ok(Some(backup))
}

#[derive(Default)]
struct Undo {
    snippet: Option<(PathBuf, Option<String>)>,
    files: Vec<(PathBuf, String, Option<PathBuf>)>,
}

impl Undo {
    fn run(self) {
        for (target, text, backup) in self.files {
            let _ = write_atomic(&target, text.as_bytes(), None, None);
            if let Some(backup) = backup {
                let _ = fs::remove_file(backup);
            }
        }
        if let Some((path, before)) = self.snippet {
            match before {
                Some(text) => {
                    let _ = write_atomic(&path, text.as_bytes(), None, None);
                }
                None => {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

/// Runs `nginx -t`. `Ok(None)` when it passes, else what nginx printed.
///
/// # Errors
/// [`SetupError`] when nginx cannot be started.
pub(super) fn test_config(host: &Host) -> Result<Option<String>, SetupError> {
    let outcome = host
        .runner
        .run(nginx_program(host).unwrap_or("nginx"), &["-t"])
        .map_err(|e| SetupError::new(format!("cannot run nginx -t: {e}")))?;
    if outcome.success {
        return Ok(None);
    }
    Ok(Some(if outcome.stderr.is_empty() {
        outcome.stdout
    } else {
        outcome.stderr
    }))
}

/// Reloads nginx with systemd, or `nginx -s reload` when that fails. Whether it worked.
pub(super) fn reload(host: &Host) -> bool {
    let done = |program: &str, args: &[&str]| {
        host.runner
            .run(program, args)
            .is_ok_and(|outcome| outcome.success)
    };
    done("systemctl", &["reload", "nginx"])
        || done(nginx_program(host).unwrap_or("nginx"), &["-s", "reload"])
}

/// Writes the snippet and the include blocks, then tests and reloads nginx. Returns whether
/// the step worked. A rejected configuration is put back and reported, not raised.
///
/// # Errors
/// [`SetupError`] when a file cannot be read or written. Whatever was changed before the
/// failure is put back.
pub(super) fn install(
    host: &Host,
    say: &mut Say,
    plan: &Plan,
    listen_port: u16,
) -> Result<bool, SetupError> {
    let mut undo = Undo::default();
    let result = write_all(host, say, plan, listen_port, &mut undo);
    let changed = match result {
        Ok(changed) => changed,
        Err(e) => {
            undo.run();
            return Err(e);
        }
    };
    if !changed {
        say.step("nginx", "already current");
        return Ok(true);
    }
    verify_and_reload(host, say, undo)
}

fn write_all(
    host: &Host,
    say: &mut Say,
    plan: &Plan,
    listen_port: u16,
    undo: &mut Undo,
) -> Result<bool, SetupError> {
    let mut changed = false;
    let path = snippet_path(host);
    let wanted = render::snippet(listen_port);
    let state = snippet_state(host, &path, &wanted)?;
    if state == SnippetState::Current {
        say.step("nginx snippet", "already current");
    } else {
        undo.snippet = Some((path.clone(), read(&path)?));
        write_snippet(host, &path, &wanted)?;
        say.step(
            "nginx snippet",
            if state == SnippetState::Missing {
                "installed"
            } else {
                "repaired"
            },
        );
        changed = true;
    }
    for (target, group) in by_file(&plan.mapped) {
        let original = read(&target)?
            .ok_or_else(|| SetupError::new(format!("{} vanished", target.display())))?;
        let mut text = original.clone();
        for server in &group {
            let first = server.first_location.ok_or_else(|| {
                SetupError::new(format!("{} has no location block", server.label()))
            })?;
            let edited = render::with_include_block(&text, server.open, server.close, first)
                .map_err(|e| SetupError::new(format!("{}: {e}", target.display())))?;
            if let Some(edited) = edited {
                text = edited;
            }
        }
        let label = group
            .iter()
            .map(|s| s.label())
            .collect::<Vec<_>>()
            .join(", ");
        if text == original {
            say.step("nginx site", &format!("{label}: already current"));
            continue;
        }
        let backup = back_up(&target, &original)?;
        undo.files.push((target.clone(), original, backup));
        write_atomic(&target, text.as_bytes(), None, None)
            .map_err(io_err(format!("cannot update {}", target.display())))?;
        say.step("nginx site", &format!("{label}: include added"));
        changed = true;
    }
    Ok(changed)
}

/// Reports the snippet, each mapped site, `nginx -t` and `trusted_proxies`, and changes
/// nothing. Returns whether all of them are in order.
///
/// # Errors
/// [`SetupError`] when a file cannot be read.
pub(super) fn check(
    host: &Host,
    say: &mut Say,
    plan: &Plan,
    listen_port: u16,
    config: &Path,
) -> Result<bool, SetupError> {
    let mut good = true;
    let mut report = |say: &mut Say, label: &str, ok: bool, state: &str| {
        good &= ok;
        say.step(label, state);
    };
    let state = snippet_state(host, &snippet_path(host), &render::snippet(listen_port))?;
    report(
        say,
        "nginx snippet",
        state == SnippetState::Current,
        match state {
            SnippetState::Current => "ok",
            SnippetState::Missing => "missing",
            SnippetState::Stale => "stale",
        },
    );
    for (target, group) in by_file(&plan.mapped) {
        let text = read(&target)?.unwrap_or_default();
        let current = group.iter().all(|s| {
            s.first_location.is_some_and(|first| {
                render::with_include_block(&text, s.open, s.close, first) == Ok(None)
            })
        });
        let label = group
            .iter()
            .map(|s| s.label())
            .collect::<Vec<_>>()
            .join(", ");
        report(
            say,
            "nginx site",
            current,
            &format!(
                "{label}: {}",
                if current { "ok" } else { "include missing" }
            ),
        );
    }
    match test_config(host)? {
        None => report(say, "nginx -t", true, "ok"),
        // Without root nginx cannot open its pid file or logs, so a failure proves nothing.
        Some(_) if !host.chown => report(say, "nginx -t", true, "needs sudo, not checked"),
        Some(printed) => {
            report(say, "nginx -t", false, "failed");
            for line in printed.lines() {
                say.note(line);
            }
        }
    }
    let proxies = read(config)?.unwrap_or_default();
    let trusted = render::with_trusted_proxies(&proxies) == Ok(None);
    report(
        say,
        "trusted_proxies",
        trusted,
        if trusted {
            "ok"
        } else {
            "missing 127.0.0.1 or ::1"
        },
    );
    Ok(good)
}

/// Takes the include block out of every file nginx reads, deletes the snippet when it carries
/// our header, then tests and reloads nginx. A rejected configuration is put back.
///
/// # Errors
/// [`SetupError`] when a file cannot be read or written. What was changed is put back.
pub(super) fn remove(host: &Host, say: &mut Say) -> Result<bool, SetupError> {
    if nginx_program(host).is_none() {
        say.step(
            "nginx",
            "not found on PATH, /usr/sbin or /sbin, nothing to remove",
        );
        return Ok(true);
    }
    let Some(layout) = Layout::read(&host.nginx_dir) else {
        say.step("nginx", "no nginx.conf, nothing to remove");
        return Ok(true);
    };
    let mut undo = Undo::default();
    let result = remove_all(host, say, &layout, &mut undo);
    let changed = match result {
        Ok(changed) => changed,
        Err(e) => {
            undo.run();
            return Err(e);
        }
    };
    if !changed {
        say.step("nginx", "no link to remove");
        return Ok(true);
    }
    verify_and_reload(host, say, undo)
}

fn remove_all(
    host: &Host,
    say: &mut Say,
    layout: &Layout,
    undo: &mut Undo,
) -> Result<bool, SetupError> {
    let mut changed = false;
    let mut done: Vec<PathBuf> = Vec::new();
    for file in &layout.files {
        let target = target_of(file);
        if done.contains(&target) {
            continue;
        }
        done.push(target.clone());
        let Some(text) = read(&target)? else { continue };
        let stripped = render::without_include_blocks(&text)
            .map_err(|e| SetupError::new(format!("{}: {e}", target.display())))?;
        if let Some(stripped) = stripped {
            undo.files.push((target.clone(), text, None));
            write_atomic(&target, stripped.as_bytes(), None, None)
                .map_err(io_err(format!("cannot update {}", target.display())))?;
            say.step(
                "nginx site",
                &format!("{}: include removed", file.display()),
            );
            changed = true;
        }
    }
    let path = snippet_path(host);
    match read(&path)? {
        Some(text) if text.starts_with(render::SNIPPET_HEADER) => {
            undo.snippet = Some((path.clone(), Some(text)));
            fs::remove_file(&path).map_err(io_err(format!("cannot remove {}", path.display())))?;
            say.step("nginx snippet", "removed");
            changed = true;
        }
        Some(_) => say.note(&format!(
            "nginx: kept {}: it does not carry the wobblescope header",
            path.display()
        )),
        None => {}
    }
    Ok(changed)
}

/// Tests the new configuration and reloads nginx. A rejected configuration is put back with
/// `undo` and what nginx printed is shown.
fn verify_and_reload(host: &Host, say: &mut Say, undo: Undo) -> Result<bool, SetupError> {
    if let Some(printed) = test_config(host)? {
        undo.run();
        say.step("nginx", "FAILED: nginx -t rejected the change; put back");
        for line in printed.lines() {
            say.note(line);
        }
        return Ok(false);
    }
    if reload(host) {
        say.step("nginx", "reloaded");
        Ok(true)
    } else {
        say.step(
            "nginx",
            "FAILED to reload; run: sudo systemctl reload nginx",
        );
        Ok(false)
    }
}
