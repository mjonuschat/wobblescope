//! Setup's conversation with Moonraker: where its files are, which sections it loaded, whether
//! a print is running, and restarting it and reading its update manager's verdict.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::time::Instant;

use crate::access::policy::parse_login_required;
use crate::config::{DEFAULT_MOONRAKER_URL, Settings, moonraker_url};
use crate::moonraker::discovery::parse_config_root;
use crate::moonraker::rpc::{Notification, Rpc, RpcError};

use super::render::SERVICE;

/// How long setup waits for each part of a conversation with Moonraker.
#[derive(Debug, Clone, Copy)]
pub struct Timings {
    /// Opening the websocket.
    pub connect: Duration,
    /// One ordinary call.
    pub request: Duration,
    /// Moonraker's answer for `machine.update.status`, which can wait on the network.
    pub status_request: Duration,
    /// The old process closing its connection and the new one answering.
    pub restart_wait: Duration,
    /// The update manager listing `wobblescope` after a restart.
    pub verify_wait: Duration,
    /// The pause between attempts.
    pub poll: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            request: Duration::from_secs(10),
            status_request: Duration::from_secs(30),
            restart_wait: Duration::from_secs(60),
            verify_wait: Duration::from_secs(60),
            poll: Duration::from_millis(500),
        }
    }
}

/// Why a conversation with Moonraker failed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoonrakerError {
    /// Moonraker did not answer.
    #[error(
        "cannot reach Moonraker at {url}: {reason}. Setup needs Moonraker to answer, because it asks Moonraker where its files are."
    )]
    Unreachable {
        /// The base URL.
        url: String,
        /// What went wrong.
        reason: String,
    },
    /// Moonraker refused setup for lack of authentication.
    #[error(
        "Moonraker refused setup as unauthorized. Rerun with --api-key <key> (use --api-key - to read the key from standard input)."
    )]
    Unauthorized,
    /// Moonraker answered, but not usefully.
    #[error("{0}")]
    Failed(String),
}

/// One config file Moonraker loaded and the sections it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    /// The file's name as Moonraker reports it.
    pub filename: String,
    /// Its section names.
    pub sections: Vec<String>,
}

/// What setup reads from Moonraker before it changes anything.
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    /// The directory of `moonraker.conf`.
    pub config_root: PathBuf,
    /// Moonraker's main config file: the first file `server.config` lists, or `moonraker.conf`
    /// when it lists none.
    pub main_config: PathBuf,
    /// The directory of `moonraker.asvc`, or why it is unknown.
    pub data_path: Result<PathBuf, String>,
    /// The config files Moonraker loaded.
    pub files: Vec<ConfigFile>,
    /// Every loaded section name.
    pub sections: Vec<String>,
    /// The `path` of the loaded `update_manager wobblescope` section, when it has one.
    pub wobblescope_path: Option<String>,
    /// Whether Moonraker requires logins.
    pub login_required: bool,
}

impl Discovery {
    /// Whether Moonraker loaded a section called `name`.
    #[must_use]
    pub fn has_section(&self, name: &str) -> bool {
        self.sections.iter().any(|s| s == name)
    }

    /// Whether Moonraker has an `[update_manager]` section, without which no entry works.
    #[must_use]
    pub fn has_update_manager(&self) -> bool {
        self.has_section("update_manager")
    }

    /// Whether Moonraker loaded an `[update_manager wobblescope]` section.
    #[must_use]
    pub fn wobblescope_loaded(&self) -> bool {
        self.has_section(&format!("update_manager {SERVICE}"))
    }
}

/// Whether a print may be running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Guard {
    /// Moonraker can restart.
    Clear,
    /// It cannot, for this reason.
    Blocked(String),
}

/// The print guard's decision from Klipper's state and `print_stats.state`. Only `shutdown` and
/// `error` prove Klipper is not printing; `startup` and `disconnected` are what Moonraker
/// reports while it reconnects to a Klipper that may still be.
#[must_use]
pub fn guard_for(klippy_state: &str, print_state: Option<&str>) -> Guard {
    match klippy_state {
        "shutdown" | "error" => Guard::Clear,
        "ready" => match print_state {
            Some("printing" | "paused") => {
                Guard::Blocked(format!("a print is {}", print_state.unwrap_or_default()))
            }
            Some(_) => Guard::Clear,
            None => Guard::Blocked("the print state could not be read".to_owned()),
        },
        other => Guard::Blocked(format!(
            "Klipper's state is {other}, so a print may be running"
        )),
    }
}

/// What the update manager says about `wobblescope`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Moonraker's `is_valid`.
    pub valid: bool,
    /// Moonraker's warnings.
    pub warnings: Vec<String>,
    /// Moonraker's anomalies.
    pub anomalies: Vec<String>,
}

/// The result of waiting for the update manager to list `wobblescope`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verification {
    /// It lists the entry.
    Listed(Verdict),
    /// It did not within the wait.
    TimedOut,
}

/// A way to talk to one Moonraker.
#[derive(Debug, Clone)]
pub struct Moonraker {
    ws_url: String,
    base_url: String,
    api_key: Option<String>,
    /// The waits.
    pub timings: Timings,
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// The `files` and `config` of a `server.config` answer: the loaded files with their sections,
/// every section name, and the `path` of the `update_manager wobblescope` section.
#[must_use]
pub fn parse_server_config(result: &Value) -> (Vec<ConfigFile>, Vec<String>, Option<String>) {
    let files: Vec<ConfigFile> = result
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| ConfigFile {
            filename: f
                .get("filename")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            sections: strings(f.get("sections").unwrap_or(&Value::Null)),
        })
        .collect();
    let mut sections: Vec<String> = files.iter().flat_map(|f| f.sections.clone()).collect();
    let config = result.get("config").and_then(Value::as_object);
    if let Some(config) = config {
        for name in config.keys() {
            if !sections.contains(name) {
                sections.push(name.clone());
            }
        }
    }
    let path = config
        .and_then(|c| c.get(&format!("update_manager {SERVICE}")))
        .and_then(|s| s.get("path"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    (files, sections, path)
}

/// The path of the main config file named `filename` by `server.config`, which is relative to
/// the config root unless it is absolute.
///
/// # Errors
/// The reason the name is unusable: it holds a control character or a `..` component.
pub fn main_config_path(config_root: &Path, filename: Option<&str>) -> Result<PathBuf, String> {
    let Some(name) = filename.filter(|n| !n.is_empty()) else {
        return Ok(config_root.join("moonraker.conf"));
    };
    let path = config_root.join(name);
    validate_path("main config file", &path)?;
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!("Moonraker's main config file {name:?} contains .."));
    }
    Ok(path)
}

/// Checks a path Moonraker reported: absolute, free of control characters.
///
/// # Errors
/// The reason the path is unusable.
pub fn validate_path(what: &str, path: &Path) -> Result<(), String> {
    let text = path.to_string_lossy();
    if !path.is_absolute() {
        return Err(format!(
            "Moonraker's {what} {text:?} is not an absolute path"
        ));
    }
    if text.chars().any(char::is_control) {
        return Err(format!(
            "Moonraker's {what} {text:?} holds a control character"
        ));
    }
    Ok(())
}

/// The data path, the directory of `moonraker.asvc`, when the standard layout holds literally:
/// the config and logs roots are real directories named `config` and `logs` under one parent,
/// and `moonraker.asvc` is a file there.
///
/// # Errors
/// The reason it cannot be told, which names the file to edit by hand.
pub fn data_path(config_root: &Path, logs_root: Option<&Path>) -> Result<PathBuf, String> {
    let hand =
        "moonraker.asvc was not found automatically; add the line `wobblescope` to it by hand";
    let real_dir = |path: &Path, name: &str| {
        path.file_name().is_some_and(|n| n == name)
            && std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
    };
    let Some(logs) = logs_root else {
        return Err(format!("Moonraker reports no logs root, so {hand}"));
    };
    let parent = config_root.parent();
    if parent.is_none()
        || parent != logs.parent()
        || !real_dir(config_root, "config")
        || !real_dir(logs, "logs")
    {
        return Err(format!(
            "the config and logs roots do not share the standard layout, so {hand}"
        ));
    }
    let parent = parent.unwrap_or(config_root);
    if parent.join("moonraker.asvc").is_file() {
        Ok(parent.to_owned())
    } else {
        Err(format!(
            "{} does not exist, so {hand}",
            parent.join("moonraker.asvc").display()
        ))
    }
}

impl Moonraker {
    /// A client for the Moonraker at `url`, or the default when there is none.
    ///
    /// # Errors
    /// The reason the URL is unusable.
    pub fn new(
        url: Option<&str>,
        api_key: Option<String>,
        timings: Timings,
    ) -> Result<Self, super::SetupError> {
        let text = url.unwrap_or(DEFAULT_MOONRAKER_URL);
        let parsed = moonraker_url(text)
            .map_err(|reason| super::SetupError::new(format!("--moonraker {text:?} {reason}")))?;
        let settings = Settings {
            moonraker_url: parsed,
            ..Settings::default()
        };
        Ok(Self {
            ws_url: settings.moonraker_ws_url(),
            base_url: text.trim_end_matches('/').to_owned(),
            api_key,
            timings,
        })
    }

    fn unreachable(&self, e: &RpcError) -> MoonrakerError {
        MoonrakerError::Unreachable {
            url: self.base_url.clone(),
            reason: e.to_string(),
        }
    }

    fn call_error(&self, method: &str, e: RpcError) -> MoonrakerError {
        if e.is_unauthorized() {
            MoonrakerError::Unauthorized
        } else {
            MoonrakerError::Failed(format!("Moonraker's {method} failed: {e}"))
        }
    }

    /// Opens a connection and identifies, with the API key when there is one.
    async fn connect(&self) -> Result<(Rpc, UnboundedReceiver<Notification>), MoonrakerError> {
        let (rpc, notes) = Rpc::connect(&self.ws_url, self.timings.connect)
            .await
            .map_err(|e| match &e {
                RpcError::Connect(m) if m.contains("HTTP 401") || m.contains("HTTP 403") => {
                    MoonrakerError::Unauthorized
                }
                _ => self.unreachable(&e),
            })?;
        let mut identify = json!({
            "client_name": "wobblescope-setup",
            "version": env!("CARGO_PKG_VERSION"),
            "type": "other",
            "url": "https://github.com/mjonuschat/wobblescope",
        });
        if let Some(key) = &self.api_key {
            identify["api_key"] = json!(key);
        }
        rpc.call(
            "server.connection.identify",
            Some(identify),
            self.timings.request,
        )
        .await
        .map_err(|e| match e {
            e if e.is_unauthorized() => MoonrakerError::Unauthorized,
            e @ (RpcError::Timeout | RpcError::Closed | RpcError::Io(_)) => self.unreachable(&e),
            e => self.call_error("identify", e),
        })?;
        Ok((rpc, notes))
    }

    async fn call(&self, rpc: &Rpc, method: &str) -> Result<Value, MoonrakerError> {
        rpc.call(method, None, self.timings.request)
            .await
            .map_err(|e| self.call_error(method, e))
    }

    /// Reads where Moonraker's files are, what it loaded and whether it requires logins.
    /// `owner_uid` must own the config directory.
    ///
    /// # Errors
    /// A [`MoonrakerError`] when Moonraker is unreachable, refuses setup or answers with
    /// something setup cannot use or trust.
    pub async fn discover(&self, owner_uid: u32) -> Result<Discovery, MoonrakerError> {
        let (rpc, _notes) = self.connect().await?;
        let roots = self.call(&rpc, "server.files.roots").await?;
        let config_root =
            parse_config_root(&roots).map_err(|e| MoonrakerError::Failed(e.to_string()))?;
        let logs_root = roots
            .as_array()
            .into_iter()
            .flatten()
            .find(|r| r.get("name").and_then(Value::as_str) == Some("logs"))
            .and_then(|r| r.get("path").and_then(Value::as_str))
            .map(PathBuf::from);
        validate_path("config root", &config_root).map_err(MoonrakerError::Failed)?;
        let metadata = std::fs::metadata(&config_root).map_err(|e| {
            MoonrakerError::Failed(format!(
                "cannot read Moonraker's config directory {}: {e}",
                config_root.display()
            ))
        })?;
        if metadata.uid() != owner_uid {
            return Err(MoonrakerError::Failed(format!(
                "Moonraker's config directory {} is not owned by the invoking user, so setup \
                 will not write there",
                config_root.display()
            )));
        }
        if let Some(logs) = &logs_root {
            validate_path("logs root", logs).map_err(MoonrakerError::Failed)?;
        }
        let data_path = data_path(&config_root, logs_root.as_deref());
        let config = self.call(&rpc, "server.config").await?;
        let (files, sections, wobblescope_path) = parse_server_config(&config);
        let main_config =
            main_config_path(&config_root, files.first().map(|f| f.filename.as_str()))
                .map_err(MoonrakerError::Failed)?;
        let login_required = rpc
            .call("access.info", None, self.timings.request)
            .await
            .ok()
            .and_then(|v| parse_login_required(&v))
            .unwrap_or(false);
        Ok(Discovery {
            config_root,
            main_config,
            data_path,
            files,
            sections,
            wobblescope_path,
            login_required,
        })
    }

    /// Whether a print may be running.
    ///
    /// # Errors
    /// A [`MoonrakerError`] when Moonraker cannot be asked at all.
    pub async fn print_guard(&self) -> Result<Guard, MoonrakerError> {
        let (rpc, _notes) = self.connect().await?;
        let info = self.call(&rpc, "server.info").await?;
        let klippy = info
            .get("klippy_state")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let print_state = if klippy == "ready" {
            let params = json!({"objects": {"print_stats": ["state"]}});
            rpc.call("printer.objects.query", Some(params), self.timings.request)
                .await
                .ok()
                .and_then(|v| {
                    v.pointer("/status/print_stats/state")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
        } else {
            None
        };
        Ok(guard_for(klippy, print_state.as_deref()))
    }

    /// Restarts Moonraker and returns once its replacement answers. It waits for the old
    /// process's connection to close first, so it never reads the old process's answers.
    ///
    /// # Errors
    /// A [`MoonrakerError`] when the restart is refused or Moonraker does not come back.
    pub async fn restart(&self) -> Result<(), MoonrakerError> {
        let (rpc, mut notes) = self.connect().await?;
        match rpc.call("server.restart", None, self.timings.request).await {
            Ok(_) | Err(RpcError::Closed | RpcError::Io(_)) => {}
            Err(e) => return Err(self.call_error("server.restart", e)),
        }
        let closed = tokio::time::timeout(self.timings.restart_wait, async {
            while notes.recv().await.is_some() {}
        })
        .await;
        if closed.is_err() {
            return Err(MoonrakerError::Failed(
                "Moonraker accepted the restart but did not close its connection".to_owned(),
            ));
        }
        let deadline = Instant::now() + self.timings.restart_wait;
        loop {
            match self.connect().await {
                Ok((rpc, _)) => {
                    if self.call(&rpc, "server.info").await.is_ok() {
                        return Ok(());
                    }
                }
                Err(MoonrakerError::Unauthorized) => return Err(MoonrakerError::Unauthorized),
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Err(MoonrakerError::Failed(format!(
                    "Moonraker did not answer again within {} seconds of its restart",
                    self.timings.restart_wait.as_secs()
                )));
            }
            tokio::time::sleep(self.timings.poll).await;
        }
    }

    /// The update manager's entry for `wobblescope` on a fresh connection, when it has one.
    async fn entry(&self) -> Result<Option<Verdict>, MoonrakerError> {
        let (rpc, _notes) = self.connect().await?;
        let status = rpc
            .call("machine.update.status", None, self.timings.status_request)
            .await
            .map_err(|e| self.call_error("machine.update.status", e))?;
        Ok(status
            .pointer(&format!("/version_info/{SERVICE}"))
            .map(|entry| Verdict {
                valid: entry.get("is_valid").and_then(Value::as_bool) == Some(true),
                warnings: strings(entry.get("warnings").unwrap_or(&Value::Null)),
                anomalies: strings(entry.get("anomalies").unwrap_or(&Value::Null)),
            }))
    }

    /// Polls the update manager until it lists `wobblescope`.
    pub async fn verify(&self) -> Verification {
        let deadline = Instant::now() + self.timings.verify_wait;
        loop {
            if let Ok(Some(verdict)) = self.entry().await {
                return Verification::Listed(verdict);
            }
            if Instant::now() >= deadline {
                return Verification::TimedOut;
            }
            tokio::time::sleep(self.timings.poll).await;
        }
    }

    /// Whether the update manager lists `wobblescope` now.
    ///
    /// # Errors
    /// A [`MoonrakerError`] when Moonraker cannot be asked.
    pub async fn lists_entry(&self) -> Result<bool, MoonrakerError> {
        Ok(self.entry().await?.is_some())
    }
}
