//! `wobblescope setup`: installs the daemon as a service next to Moonraker, and checks or
//! removes that install.

pub mod host;
pub mod install;
pub mod moonraker;
pub mod render;

use std::fmt;

use crate::cli::{ApiKey, SetupArgs};

/// Why setup stopped. The text is for the person at the terminal and never holds an API key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupError(String);

impl SetupError {
    /// An error with this message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SetupError {}

/// What `setup` was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Install or repair.
    Install,
    /// Report only.
    Check,
    /// Take the install apart.
    Remove,
}

impl Mode {
    /// The mode the flags ask for.
    #[must_use]
    pub fn of(args: &SetupArgs) -> Self {
        if args.check {
            Self::Check
        } else if args.remove {
            Self::Remove
        } else {
            Self::Install
        }
    }
}

/// The API key to use: the flag's value, or one line of `input` when the flag is `-`.
///
/// # Errors
/// [`SetupError`] when standard input cannot be read or holds no key.
pub fn resolve_api_key(
    key: Option<&ApiKey>,
    input: &mut impl std::io::BufRead,
) -> Result<Option<String>, SetupError> {
    let Some(key) = key else { return Ok(None) };
    if key.expose() != "-" {
        return Ok(Some(key.expose().to_owned()));
    }
    let mut line = String::new();
    input.read_line(&mut line).map_err(|e| {
        SetupError::new(format!("cannot read the API key from standard input: {e}"))
    })?;
    let line = line.trim_end_matches(['\n', '\r']);
    if line.is_empty() {
        return Err(SetupError::new("standard input held no API key"));
    }
    Ok(Some(line.to_owned()))
}

/// What the flags decided, with the key resolved.
#[derive(Clone)]
pub struct Options {
    /// What to do.
    pub mode: Mode,
    /// Where Moonraker is, when the flag names it.
    pub moonraker: Option<String>,
    /// Moonraker's API key, when given.
    pub api_key: Option<String>,
}

/// Runs `mode` on `host` against Moonraker, printing to `out`. Returns whether everything is
/// in order.
///
/// # Errors
/// [`SetupError`] for a failure that stops the run.
pub async fn execute(
    host: &host::Host,
    moonraker: &moonraker::Moonraker,
    opts: &Options,
    out: &mut dyn std::io::Write,
) -> Result<bool, SetupError> {
    match opts.mode {
        Mode::Install => install::install(host, moonraker, opts, out).await,
        Mode::Check => install::check(host, moonraker, opts, out).await,
        Mode::Remove => install::remove(host, moonraker, opts, out).await,
    }
}

/// Runs `setup` and returns the process exit code.
pub async fn run(args: SetupArgs) -> std::process::ExitCode {
    let outcome = async {
        let mode = Mode::of(&args);
        let host = host::Host::detect(mode != Mode::Check)?;
        let api_key = resolve_api_key(args.api_key.as_ref(), &mut std::io::stdin().lock())?;
        let opts = Options {
            mode,
            moonraker: args.moonraker,
            api_key: api_key.clone(),
        };
        let moonraker = moonraker::Moonraker::new(
            opts.moonraker.as_deref(),
            api_key,
            moonraker::Timings::default(),
        )?;
        execute(&host, &moonraker, &opts, &mut std::io::stdout()).await
    }
    .await;
    match outcome {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
