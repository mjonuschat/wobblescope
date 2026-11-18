//! Setup's conversation with Moonraker: where its files are and whether it requires logins.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::access::policy::parse_login_required;
use crate::config::{DEFAULT_MOONRAKER_URL, Settings, moonraker_url};
use crate::moonraker::discovery::parse_config_root;
use crate::moonraker::rpc::{Notification, Rpc, RpcError};
use crate::moonraker::tls::Tls;

/// How long setup waits for each part of a conversation with Moonraker.
#[derive(Debug, Clone, Copy)]
pub struct Timings {
    /// Opening the websocket.
    pub connect: Duration,
    /// One ordinary call.
    pub request: Duration,
}

impl Default for Timings {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(5),
            request: Duration::from_secs(10),
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

/// What setup reads from Moonraker before it changes anything.
#[derive(Debug, Clone, PartialEq)]
pub struct Discovery {
    /// The directory of `moonraker.conf`.
    pub config_root: PathBuf,
    /// The directory of `moonraker.asvc`, or why it is unknown.
    pub data_path: Result<PathBuf, String>,
    /// Whether Moonraker requires logins.
    pub login_required: bool,
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
        if parsed.scheme_str() == Some("https") {
            return Err(super::SetupError::new(format!(
                "--moonraker {text:?} must be a plain http URL: setup edits Moonraker's files and \
                 runs on its host"
            )));
        }
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
        let (rpc, notes) = Rpc::connect(&self.ws_url, self.timings.connect, &Tls::default())
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

    /// Reads where Moonraker's files are and whether it requires logins.
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
        let login_required = rpc
            .call("access.info", None, self.timings.request)
            .await
            .ok()
            .and_then(|v| parse_login_required(&v))
            .unwrap_or(false);
        Ok(Discovery {
            config_root,
            data_path,
            login_required,
        })
    }
}
