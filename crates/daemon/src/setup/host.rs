//! What setup needs from the machine it runs on: who invoked it, atomic file writes and the
//! command runner for `systemctl`. Tests swap the runner and point the paths at a temporary
//! directory.

use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use super::SetupError;

/// The user who ran `sudo wobblescope setup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invoker {
    /// The account name.
    pub name: String,
    /// The account's user id.
    pub uid: u32,
    /// The account's group id.
    pub gid: u32,
    /// The account's home directory.
    pub home: PathBuf,
}

/// Whether `name` is a plain account name: letters, digits, `_` and `-`, not starting with `-`.
#[must_use]
pub fn valid_user_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

struct PasswdEntry {
    name: String,
    uid: u32,
    gid: u32,
    home: PathBuf,
}

fn passwd_entries(passwd: &str) -> impl Iterator<Item = PasswdEntry> + '_ {
    passwd.lines().filter_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        let _password = fields.next()?;
        let uid = fields.next()?.parse().ok()?;
        let gid = fields.next()?.parse().ok()?;
        let _gecos = fields.next()?;
        let home = fields.next()?;
        Some(PasswdEntry {
            name: name.to_owned(),
            uid,
            gid,
            home: PathBuf::from(home),
        })
    })
}

impl Invoker {
    /// The user named by `sudo`, from the text of `/etc/passwd`. `SUDO_UID` and `SUDO_GID`
    /// override the account's ids when they are numbers.
    ///
    /// # Errors
    /// [`SetupError`] when the name is missing, is not a plain account name or is not in the
    /// file.
    pub fn from_sudo(
        passwd: &str,
        sudo_user: Option<&str>,
        sudo_uid: Option<&str>,
        sudo_gid: Option<&str>,
    ) -> Result<Self, SetupError> {
        let Some(name) = sudo_user else {
            return Err(SetupError::new(
                "setup must be run with sudo; run: sudo wobblescope setup",
            ));
        };
        if !valid_user_name(name) {
            return Err(SetupError::new("SUDO_USER is not a valid account name"));
        }
        let entry = passwd_entries(passwd)
            .find(|e| e.name == name)
            .ok_or_else(|| SetupError::new(format!("no account named {name} in /etc/passwd")))?;
        Ok(Self {
            name: entry.name,
            uid: sudo_uid.and_then(|v| v.parse().ok()).unwrap_or(entry.uid),
            gid: sudo_gid.and_then(|v| v.parse().ok()).unwrap_or(entry.gid),
            home: entry.home,
        })
    }

    /// The user with id `uid`, for `--check`, which runs without sudo.
    ///
    /// # Errors
    /// [`SetupError`] when no account has that id.
    pub fn from_uid(passwd: &str, uid: u32) -> Result<Self, SetupError> {
        let entry = passwd_entries(passwd)
            .find(|e| e.uid == uid)
            .ok_or_else(|| SetupError::new(format!("no account with user id {uid}")))?;
        Ok(Self {
            name: entry.name,
            uid: entry.uid,
            gid: entry.gid,
            home: entry.home,
        })
    }
}

/// Writes `contents` to `path` through a temporary file in the same directory and a rename, so
/// a reader never sees half a file. A symlink is followed to its target. `mode` and `owner`
/// default to those of the file being replaced; a new file with neither gets `0644`.
///
/// # Errors
/// The I/O error that stopped the write; the temporary file is removed.
pub fn write_atomic(
    path: &Path,
    contents: &[u8],
    mode: Option<u32>,
    owner: Option<(u32, u32)>,
) -> io::Result<()> {
    let target = if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        fs::canonicalize(path)?
    } else {
        path.to_owned()
    };
    let existing = fs::metadata(&target).ok();
    let mode = mode
        .or_else(|| existing.as_ref().map(|m| m.mode() & 0o7777))
        .unwrap_or(0o644);
    let owner = owner.or_else(|| existing.as_ref().map(|m| (m.uid(), m.gid())));
    let dir = target
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent directory"))?;
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::other("path has no file name"))?;
    let temp = dir.join(format!(".{}.wobblescope-tmp", name.to_string_lossy()));
    let _ = fs::remove_file(&temp);
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(contents)?;
        if let Some((uid, gid)) = owner {
            std::os::unix::fs::fchown(&file, Some(uid), Some(gid))?;
        }
        file.set_permissions(fs::Permissions::from_mode(mode))?;
        file.sync_all()?;
        fs::rename(&temp, &target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// What a finished command said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Whether it exited with status zero.
    pub success: bool,
    /// What it wrote to standard output, trimmed.
    pub stdout: String,
    /// What it wrote to standard error, trimmed.
    pub stderr: String,
}

/// Runs the commands setup needs. The real one runs them; tests record them.
pub trait Runner: Send + Sync {
    /// Runs `program` with `args` and waits for it.
    ///
    /// # Errors
    /// The I/O error when the program could not be started.
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Outcome>;
}

/// Runs commands on this machine.
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> io::Result<Outcome> {
        let output = Command::new(program).args(args).output()?;
        Ok(Outcome {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

/// The machine, with its seams: who runs setup, where it puts the unit, which binary it copies
/// and how it runs `systemctl`.
pub struct Host {
    /// The user the service runs as.
    pub invoker: Invoker,
    /// Whether files setup creates are chowned to the invoker (true under sudo).
    pub chown: bool,
    /// The directory systemd reads units from.
    pub unit_dir: PathBuf,
    /// The running binary, which setup copies.
    pub binary: PathBuf,
    /// The command runner.
    pub runner: Box<dyn Runner>,
    /// How long a started service must stay active before setup calls it started.
    pub settle: Duration,
}

impl Host {
    /// The host setup runs on: the real `/etc/passwd`, this binary and the real `systemctl`.
    ///
    /// # Errors
    /// [`SetupError`] when the invoking user or the running binary cannot be found, or when
    /// `needs_root` and the process is not root run through sudo.
    pub fn detect(needs_root: bool) -> Result<Self, SetupError> {
        let passwd = fs::read_to_string("/etc/passwd")
            .map_err(|e| SetupError::new(format!("cannot read /etc/passwd: {e}")))?;
        let euid = rustix::process::geteuid().as_raw();
        let env = |key: &str| std::env::var(key).ok();
        let invoker = if needs_root {
            if euid != 0 {
                return Err(SetupError::new(
                    "setup must be run with sudo; run: sudo wobblescope setup",
                ));
            }
            Invoker::from_sudo(
                &passwd,
                env("SUDO_USER").as_deref(),
                env("SUDO_UID").as_deref(),
                env("SUDO_GID").as_deref(),
            )?
        } else {
            match env("SUDO_USER") {
                Some(name) if euid == 0 => Invoker::from_sudo(
                    &passwd,
                    Some(&name),
                    env("SUDO_UID").as_deref(),
                    env("SUDO_GID").as_deref(),
                )?,
                _ => Invoker::from_uid(&passwd, euid)?,
            }
        };
        let binary = std::env::current_exe()
            .and_then(|exe| exe.canonicalize())
            .map_err(|e| SetupError::new(format!("cannot resolve the wobblescope binary: {e}")))?;
        Ok(Self {
            invoker,
            chown: euid == 0,
            unit_dir: PathBuf::from("/etc/systemd/system"),
            binary,
            runner: Box::new(SystemRunner),
            settle: Duration::from_secs(5),
        })
    }

    /// The owner new files get, when setup chowns them.
    #[must_use]
    pub fn owner(&self) -> Option<(u32, u32)> {
        self.chown.then_some((self.invoker.uid, self.invoker.gid))
    }

    /// Runs `systemctl` with `args`.
    ///
    /// # Errors
    /// [`SetupError`] when it cannot start; the outcome says whether it succeeded.
    pub fn systemctl(&self, args: &[&str]) -> Result<Outcome, SetupError> {
        self.runner
            .run("systemctl", args)
            .map_err(|e| SetupError::new(format!("cannot run systemctl {}: {e}", args.join(" "))))
    }

    /// Runs `systemctl` and fails when it exits non-zero.
    ///
    /// # Errors
    /// [`SetupError`] naming the command and what it printed.
    pub fn systemctl_ok(&self, args: &[&str]) -> Result<(), SetupError> {
        let outcome = self.systemctl(args)?;
        if outcome.success {
            Ok(())
        } else {
            Err(SetupError::new(format!(
                "systemctl {} failed: {}",
                args.join(" "),
                outcome.stderr
            )))
        }
    }
}
