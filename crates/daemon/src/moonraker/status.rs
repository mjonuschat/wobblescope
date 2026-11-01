//! What the daemon knows about Moonraker and Klipper at a point in time.

use serde::Serialize;

/// The state of the daemon's connection to Moonraker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MoonrakerState {
    /// No attempt has finished yet.
    Connecting,
    /// Connected and identified.
    Connected,
    /// The last attempt or the connection failed.
    Unreachable,
    /// Moonraker refused the daemon for lack of authentication.
    Unauthorized,
}

impl MoonrakerState {
    /// The state's name as the API and the log spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Unreachable => "unreachable",
            Self::Unauthorized => "unauthorized",
        }
    }
}

/// Klipper's state as Moonraker reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KlippyState {
    /// Not known: no connection to Moonraker, or a state Moonraker named that this daemon
    /// does not.
    Unknown,
    /// Moonraker cannot reach Klipper.
    Disconnected,
    /// Klipper is starting.
    Startup,
    /// Klipper is ready.
    Ready,
    /// Klipper stopped on an error.
    Error,
    /// Klipper shut down.
    Shutdown,
}

impl KlippyState {
    /// The state's name as the API and the log spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Disconnected => "disconnected",
            Self::Startup => "startup",
            Self::Ready => "ready",
            Self::Error => "error",
            Self::Shutdown => "shutdown",
        }
    }

    /// The state for Moonraker's `klippy_state` text. A name this daemon does not know is
    /// [`KlippyState::Unknown`].
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text {
            "disconnected" => Self::Disconnected,
            "startup" => Self::Startup,
            "ready" => Self::Ready,
            "error" => Self::Error,
            "shutdown" => Self::Shutdown,
            _ => Self::Unknown,
        }
    }

    /// Whether Klipper has a message worth asking for in this state.
    #[must_use]
    pub fn has_message(self) -> bool {
        matches!(self, Self::Startup | Self::Error | Self::Shutdown)
    }
}

/// Moonraker's side of the status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoonrakerStatus {
    /// The connection's state.
    pub state: MoonrakerState,
    /// Why, when the state is a failure.
    pub message: Option<String>,
    /// Moonraker's version, once it has answered.
    pub version: Option<String>,
}

/// Klipper's side of the status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KlippyStatus {
    /// Klipper's state.
    pub state: KlippyState,
    /// Klipper's own message for the state, when it has one.
    pub message: Option<String>,
}

/// The daemon's view of Moonraker and Klipper. It holds no secret: no field can carry the API
/// key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// The connection to Moonraker.
    pub moonraker: MoonrakerStatus,
    /// Klipper behind it.
    pub klippy: KlippyStatus,
}

impl Default for Status {
    fn default() -> Self {
        Self {
            moonraker: MoonrakerStatus {
                state: MoonrakerState::Connecting,
                message: None,
                version: None,
            },
            klippy: KlippyStatus {
                state: KlippyState::Unknown,
                message: None,
            },
        }
    }
}

/// The one log line for a change of `subject`'s `state`. Control characters in `message`, which
/// Moonraker or Klipper supplied, are escaped so the text cannot forge further log lines. It
/// takes no secret, so it cannot hold one.
#[must_use]
pub fn state_line(subject: &str, state: &str, message: Option<&str>) -> String {
    let mut line = format!("{subject}: {state}");
    if let Some(message) = message {
        line.push_str(": ");
        for c in message.chars() {
            if c.is_control() {
                line.extend(c.escape_default());
            } else {
                line.push(c);
            }
        }
    }
    line
}
