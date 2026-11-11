//! The daemon's config file: the storage settings and where Moonraker is.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use axum::http::Uri;
use serde::Deserialize;

use crate::access::policy::{Net, NetError};

const MIB: u64 = 1024 * 1024;
const DEFAULT_CAP_MIB: u64 = 2048;
const DEFAULT_HEADROOM_MIB: u64 = 256;
const DEFAULT_MAX_SEGMENT_SECONDS: u32 = 600;
const DEFAULT_LISTEN: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 7126);
/// The Moonraker URL used when the config names none.
pub const DEFAULT_MOONRAKER_URL: &str = "http://127.0.0.1:7125";

/// Why the config could not be used.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file could not be read.
    #[error("cannot read config {}: {source}", path.display())]
    Read {
        /// The config file.
        path: PathBuf,
        /// The I/O error.
        source: std::io::Error,
    },
    /// The file is not valid TOML, has an unknown key or a value of the wrong type. The text is
    /// the parser's message with its position, never the offending line, which could hold the
    /// API key.
    #[error("invalid config {}: {message} at line {line}, column {column}{}", path.display(), key_note(key))]
    Parse {
        /// The config file.
        path: PathBuf,
        /// The parser's message.
        message: String,
        /// The 1-based line of the error.
        line: usize,
        /// The 1-based column of the error.
        column: usize,
        /// The key on that line, when the line starts with one.
        key: Option<String>,
    },
    /// A value is out of range.
    #[error("invalid config {}: {key} {reason}", path.display())]
    Invalid {
        /// The config file.
        path: PathBuf,
        /// The key.
        key: &'static str,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// One entry of a list is out of range. The text is the entry, which holds no secret.
    #[error("invalid config {}: {key} entry {entry:?} {reason}", path.display())]
    InvalidEntry {
        /// The config file.
        path: PathBuf,
        /// The key.
        key: &'static str,
        /// The entry's text.
        entry: String,
        /// What is wrong with it.
        reason: &'static str,
    },
    /// The config root has no parent directory to put the data directory beside.
    #[error("cannot derive a data directory from config root {:?}: it has no parent", .0)]
    NoParent(PathBuf),
}

fn key_note(key: &Option<String>) -> String {
    key.as_ref()
        .map_or_else(String::new, |k| format!(" (key {k})"))
}

/// Builds the parse error from the parser's, keeping only its message, its position and the
/// line's key.
fn parse_error(path: &Path, text: &str, e: &toml::de::Error) -> ConfigError {
    let offset = e.span().map_or(0, |s| s.start).min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = text[line_start..offset].chars().count() + 1;
    let line_text = text[line_start..].lines().next().unwrap_or_default();
    let key = line_text
        .split_once('=')
        .map(|(k, _)| k.trim())
        .filter(|k| {
            !k.is_empty()
                && k.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(str::to_owned);
    ConfigError::Parse {
        path: path.to_owned(),
        message: e.message().to_owned(),
        line,
        column,
        key,
    }
}

/// The keys of the file, in the units a person writes.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    storage_dir: Option<PathBuf>,
    retention_cap_mib: Option<u64>,
    free_space_headroom_mib: Option<u64>,
    moonraker_url: Option<String>,
    moonraker_api_key: Option<String>,
    max_segment_seconds: Option<u32>,
    stream_idle_updates_per_second: Option<f64>,
    stream_printing_updates_per_second: Option<f64>,
    stream_max_updates_per_second: Option<f64>,
    stream_default_max_hz: Option<f64>,
    stream_max_hz: Option<f64>,
    listen: Option<String>,
    trusted_proxies: Option<Vec<String>>,
    allowed_hosts: Option<Vec<String>>,
}

/// The storage settings, with sizes in bytes.
#[derive(Clone, PartialEq)]
pub struct Settings {
    /// Where sessions and pins live, when the config names it.
    pub storage_dir: Option<PathBuf>,
    /// Total size of retained sessions above which retention evicts.
    pub retention_cap_bytes: u64,
    /// Free space that must remain on the sessions filesystem after a test's estimate.
    pub free_space_headroom_bytes: u64,
    /// The base URL of Moonraker: plain `http`, a host, and a path that is its route prefix.
    pub moonraker_url: Uri,
    /// The API key sent when identifying to Moonraker, when the config sets one.
    pub moonraker_api_key: Option<String>,
    /// The longest one `TEST_RESONANCES` command may be expected to run, in seconds.
    pub max_segment_seconds: u32,
    /// The stream view's display updates per second while no test or print runs.
    pub stream_idle_updates_per_second: f64,
    /// The stream view's display updates per second while a print runs.
    pub stream_printing_updates_per_second: f64,
    /// The most display updates per second a viewer may ask for.
    pub stream_max_updates_per_second: f64,
    /// The top of the stream view's frequency range when the viewer does not set one, in Hz.
    pub stream_default_max_hz: f64,
    /// The highest top of the frequency range a viewer may ask for, in Hz.
    pub stream_max_hz: f64,
    /// The address the daemon listens on.
    pub listen: SocketAddr,
    /// The addresses and networks whose `X-Forwarded-For` the daemon believes.
    pub trusted_proxies: Vec<Net>,
    /// The host names, lowercase, that the `Host` check admits besides the ones Moonraker's
    /// lists imply.
    pub allowed_hosts: Vec<String>,
}

impl Settings {
    /// The websocket URL of Moonraker: the base URL with the `ws` scheme and `/websocket`
    /// appended to its path.
    #[must_use]
    pub fn moonraker_ws_url(&self) -> String {
        format!("{}/websocket", self.moonraker_ws_base())
    }

    /// The URL of Moonraker's bridge to Klipper's own socket, without its token.
    #[must_use]
    pub fn moonraker_bridge_base(&self) -> String {
        format!("{}/klippysocket", self.moonraker_ws_base())
    }

    /// The URL of the bridge with the one-shot `token` in its query.
    #[must_use]
    pub fn moonraker_bridge_url(&self, token: &str) -> String {
        format!("{}?token={token}", self.moonraker_bridge_base())
    }

    fn moonraker_ws_base(&self) -> String {
        let authority = self
            .moonraker_url
            .authority()
            .map_or("", axum::http::uri::Authority::as_str);
        let prefix = self.moonraker_url.path().trim_end_matches('/');
        format!("ws://{authority}{prefix}")
    }
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("storage_dir", &self.storage_dir)
            .field("retention_cap_bytes", &self.retention_cap_bytes)
            .field("free_space_headroom_bytes", &self.free_space_headroom_bytes)
            .field("moonraker_url", &self.moonraker_url)
            .field("max_segment_seconds", &self.max_segment_seconds)
            .field(
                "stream_idle_updates_per_second",
                &self.stream_idle_updates_per_second,
            )
            .field(
                "stream_printing_updates_per_second",
                &self.stream_printing_updates_per_second,
            )
            .field(
                "stream_max_updates_per_second",
                &self.stream_max_updates_per_second,
            )
            .field("stream_default_max_hz", &self.stream_default_max_hz)
            .field("stream_max_hz", &self.stream_max_hz)
            .field("listen", &self.listen)
            .field("trusted_proxies", &self.trusted_proxies)
            .field("allowed_hosts", &self.allowed_hosts)
            .field(
                "moonraker_api_key",
                &self.moonraker_api_key.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            storage_dir: None,
            retention_cap_bytes: DEFAULT_CAP_MIB * MIB,
            free_space_headroom_bytes: DEFAULT_HEADROOM_MIB * MIB,
            moonraker_url: Uri::from_static(DEFAULT_MOONRAKER_URL),
            moonraker_api_key: None,
            max_segment_seconds: DEFAULT_MAX_SEGMENT_SECONDS,
            stream_idle_updates_per_second: 10.0,
            stream_printing_updates_per_second: 5.0,
            stream_max_updates_per_second: 20.0,
            stream_default_max_hz: 200.0,
            stream_max_hz: 1600.0,
            listen: DEFAULT_LISTEN,
            trusted_proxies: Vec::new(),
            allowed_hosts: Vec::new(),
        }
    }
}

/// Checks `moonraker_url` and returns it parsed.
pub fn moonraker_url(text: &str) -> Result<Uri, &'static str> {
    if text.contains('#') {
        return Err("must not have a fragment");
    }
    let uri: Uri = text
        .parse()
        .map_err(|_| "must be a URL like http://127.0.0.1:7125")?;
    match uri.scheme_str() {
        Some("http") => {}
        Some("https") => return Err("https is not supported yet; use a plain http URL"),
        _ => return Err("must start with http://"),
    }
    let Some(authority) = uri.authority() else {
        return Err("must name a host");
    };
    if authority.as_str().contains('@') {
        return Err("must not contain credentials");
    }
    if authority.host().is_empty() {
        return Err("must name a host");
    }
    if uri.query().is_some() {
        return Err("must not have a query");
    }
    Ok(uri)
}

/// A host name for the allowlist: letters, digits, `-` and `.`, lowercased.
fn allowed_host(text: &str) -> Result<String, &'static str> {
    if text.is_empty() {
        return Err("must not be empty");
    }
    if !text
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    {
        return Err("must be a bare host name: no scheme, port or path");
    }
    Ok(text.to_lowercase())
}

/// The least display update rate, shared by config validation and the stream clamp.
pub(crate) const MIN_UPDATES_PER_SECOND: f64 = 0.5;

/// The value or its default, which must be finite and at least `min`; `too_small` is the
/// reason given when it is not.
fn finite_at_least(
    value: Option<f64>,
    default: f64,
    min: f64,
    too_small: &'static str,
) -> Result<f64, &'static str> {
    let value = value.unwrap_or(default);
    if !value.is_finite() {
        return Err("must be a finite number");
    }
    if value < min {
        return Err(too_small);
    }
    Ok(value)
}

/// A display update rate: finite and at least [`MIN_UPDATES_PER_SECOND`].
fn rate(value: Option<f64>, default: f64) -> Result<f64, &'static str> {
    finite_at_least(
        value,
        default,
        MIN_UPDATES_PER_SECOND,
        "must be at least 0.5",
    )
}

/// Loads the settings from `path`, or returns the defaults when there is no path.
///
/// # Errors
/// A [`ConfigError`] naming the file, and the key for a bad value.
pub fn load(path: Option<&Path>) -> Result<Settings, ConfigError> {
    let Some(path) = path else {
        return Ok(Settings::default());
    };
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_owned(),
        source,
    })?;
    let raw: RawConfig = toml::from_str(&text).map_err(|e| parse_error(path, &text, &e))?;
    let invalid = |key, reason| ConfigError::Invalid {
        path: path.to_owned(),
        key,
        reason,
    };
    if raw.storage_dir.as_deref().is_some_and(|d| !d.is_absolute()) {
        return Err(invalid("storage_dir", "must be an absolute path"));
    }
    if raw.retention_cap_mib == Some(0) {
        return Err(invalid("retention_cap_mib", "must be above 0"));
    }
    if raw.max_segment_seconds == Some(0) {
        return Err(invalid("max_segment_seconds", "must be above 0"));
    }
    let defaults = Settings::default();
    let stream_idle = rate(
        raw.stream_idle_updates_per_second,
        defaults.stream_idle_updates_per_second,
    )
    .map_err(|reason| invalid("stream_idle_updates_per_second", reason))?;
    let stream_printing = rate(
        raw.stream_printing_updates_per_second,
        defaults.stream_printing_updates_per_second,
    )
    .map_err(|reason| invalid("stream_printing_updates_per_second", reason))?;
    let stream_max_rate = rate(
        raw.stream_max_updates_per_second,
        defaults.stream_max_updates_per_second,
    )
    .map_err(|reason| invalid("stream_max_updates_per_second", reason))?;
    let stream_default_hz = finite_at_least(
        raw.stream_default_max_hz,
        defaults.stream_default_max_hz,
        1.0,
        "must be at least 1",
    )
    .map_err(|reason| invalid("stream_default_max_hz", reason))?;
    let stream_max_hz = finite_at_least(
        raw.stream_max_hz,
        defaults.stream_max_hz,
        1.0,
        "must be at least 1",
    )
    .map_err(|reason| invalid("stream_max_hz", reason))?;
    if stream_max_rate < stream_idle.max(stream_printing) {
        return Err(invalid(
            "stream_max_updates_per_second",
            "must be at least the idle and printing update rates",
        ));
    }
    if stream_max_hz < stream_default_hz {
        return Err(invalid(
            "stream_max_hz",
            "must be at least stream_default_max_hz",
        ));
    }
    if raw.moonraker_api_key.as_deref() == Some("") {
        return Err(invalid("moonraker_api_key", "must not be empty"));
    }
    let entry = |key, entry: &str, reason| ConfigError::InvalidEntry {
        path: path.to_owned(),
        key,
        entry: entry.to_owned(),
        reason,
    };
    let listen = match raw.listen {
        Some(text) => text.parse().map_err(|_| {
            invalid(
                "listen",
                "must be an IP address and port like 0.0.0.0:7126; a host name is not allowed",
            )
        })?,
        None => DEFAULT_LISTEN,
    };
    let mut trusted_proxies = Vec::new();
    for text in raw.trusted_proxies.iter().flatten() {
        trusted_proxies.push(Net::parse(text).map_err(|e| {
            entry(
                "trusted_proxies",
                text,
                match e {
                    NetError::HostBits => "is a network with host bits set",
                    NetError::Invalid => "is not an IP address or network",
                },
            )
        })?);
    }
    let mut allowed_hosts = Vec::new();
    for text in raw.allowed_hosts.iter().flatten() {
        allowed_hosts
            .push(allowed_host(text).map_err(|reason| entry("allowed_hosts", text, reason))?);
    }
    let moonraker_url = match raw.moonraker_url {
        Some(text) => moonraker_url(&text).map_err(|reason| invalid("moonraker_url", reason))?,
        None => defaults.moonraker_url,
    };
    Ok(Settings {
        listen,
        trusted_proxies,
        allowed_hosts,
        moonraker_url,
        moonraker_api_key: raw.moonraker_api_key,
        max_segment_seconds: raw
            .max_segment_seconds
            .unwrap_or(defaults.max_segment_seconds),
        stream_idle_updates_per_second: stream_idle,
        stream_printing_updates_per_second: stream_printing,
        stream_max_updates_per_second: stream_max_rate,
        stream_default_max_hz: stream_default_hz,
        stream_max_hz,
        storage_dir: raw.storage_dir,
        retention_cap_bytes: raw
            .retention_cap_mib
            .map_or(defaults.retention_cap_bytes, |m| m.saturating_mul(MIB)),
        free_space_headroom_bytes: raw
            .free_space_headroom_mib
            .map_or(defaults.free_space_headroom_bytes, |m| {
                m.saturating_mul(MIB)
            }),
    })
}

/// The data directory Moonraker's layout implies: `wobblescope` beside the config root, which
/// Moonraker reports as e.g. `~/printer_data/config`.
///
/// # Errors
/// [`ConfigError::NoParent`] for `/` or an empty path.
pub fn default_data_dir(config_root: &Path) -> Result<PathBuf, ConfigError> {
    config_root
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.join("wobblescope"))
        .ok_or_else(|| ConfigError::NoParent(config_root.to_owned()))
}
