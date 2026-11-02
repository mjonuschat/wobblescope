//! `wobblescope setup`: installs the daemon as a service next to Moonraker, and checks or
//! removes that install.

pub mod host;

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

/// Runs `setup` and returns the process exit code.
pub async fn run(args: SetupArgs) -> std::process::ExitCode {
    let outcome = async {
        let mode = Mode::of(&args);
        let host = host::Host::detect(mode != Mode::Check)?;
        let _key = resolve_api_key(args.api_key.as_ref(), &mut std::io::stdin().lock())?;
        let _ = host;
        Err::<(), _>(SetupError::new("setup is not implemented yet"))
    }
    .await;
    match outcome {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
