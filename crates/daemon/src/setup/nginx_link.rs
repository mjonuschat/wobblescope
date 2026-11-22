//! The nginx step of setup: a snippet that serves the daemon under `/wobblescope/`, and one
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

/// Where the snippet lives under the nginx directory.
const SNIPPET: &str = "snippets/wobblescope.conf";

/// Prints the one-line reasons nothing can be done, and returns the plan otherwise.
/// Not finding nginx or a site is not an error.
pub(super) fn find(host: &Host, say: &mut Say, moonraker_port: u16) -> Option<Plan> {
    if host.runner.run("nginx", &["-v"]).is_err() {
        say.step("nginx", "not installed, skipped");
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

/// The snippet's path.
#[must_use]
pub(super) fn snippet_path(host: &Host) -> PathBuf {
    host.nginx_dir.join(SNIPPET)
}

fn target_of(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// What the snippet file is now.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SnippetState {
    /// Not there.
    Missing,
    /// A regular file with the wanted text and the wanted mode and owner.
    Current,
    /// There, but differs, is a symlink, or has the wrong mode or owner.
    Stale,
}

/// Compares the snippet at `path` with `wanted`.
///
/// # Errors
/// [`SetupError`] when the file cannot be read.
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
fn back_up(target: &Path, text: &str) -> Result<Option<PathBuf>, SetupError> {
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

/// What to put back when nginx rejects the change.
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
        .run("nginx", &["-t"])
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
    done("systemctl", &["reload", "nginx"]) || done("nginx", &["-s", "reload"])
}

/// Writes the snippet and the include blocks, then tests and reloads nginx. Returns whether
/// the step worked; a rejected configuration is put back and reported, not raised.
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
