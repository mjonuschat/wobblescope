//! The command line: no subcommand runs the daemon, `setup` installs it next to Moonraker.

use std::fmt;
use std::str::FromStr;

use clap::{Args, Parser, Subcommand};

/// The command line of the `wobblescope` binary.
#[derive(Debug, Parser)]
#[command(name = "wobblescope", version, about)]
pub struct Cli {
    /// What to run. Without one the daemon starts.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Install the daemon as a service next to Moonraker, or check or remove that install
    Setup(SetupArgs),
}

/// The options of `setup`.
#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Report the state of each piece and change nothing
    #[arg(long, conflicts_with = "remove")]
    pub check: bool,
    /// Stop and remove the service and its Moonraker entries; the config file and the recorded
    /// sessions stay
    #[arg(long)]
    pub remove: bool,
    /// Where Moonraker listens, when not at the default
    #[arg(long, value_name = "URL")]
    pub moonraker: Option<String>,
    /// Moonraker's API key, for when Moonraker refuses setup; `-` reads it from standard input
    #[arg(long, value_name = "KEY")]
    pub api_key: Option<ApiKey>,
}

/// An API key that never shows in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// The key's text, or `-` when it is to be read from standard input.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Wraps a key's text.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }
}

impl FromStr for ApiKey {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        if text.is_empty() {
            return Err("the key is empty".to_owned());
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}
