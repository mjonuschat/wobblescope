//! The files setup owns, as text: the service unit, the daemon's config and the service list.
//! Nothing here does I/O.

use std::path::Path;

/// The unit's name and the service-list entry.
pub const SERVICE: &str = "wobblescope";

/// Escapes `value` for a double-quoted `ExecStart=` or `Environment=` word: doubles `%`, which
/// systemd expands, and backslash-escapes `\` and `"`. Newlines become spaces.
fn escape_exec(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '%' => escaped.push_str("%%"),
            '\n' | '\r' => escaped.push(' '),
            other => escaped.push(other),
        }
    }
    escaped
}

/// The systemd unit that runs `binary` as `user`, reading its settings from `config`.
#[must_use]
pub fn unit(user: &str, binary: &Path, config: &Path) -> String {
    let binary = escape_exec(&binary.display().to_string());
    let config = escape_exec(&config.display().to_string());
    format!(
        "[Unit]\nDescription=Wobblescope resonance analysis service\n\
         After=moonraker.service\nWants=moonraker.service\n\n\
         [Service]\nType=simple\nUser={user}\n\
         Environment=\"WOBBLESCOPE_CONFIG={config}\"\n\
         ExecStart=\"{binary}\"\nRestart=always\nRestartSec=5\n\n\
         [Install]\nWantedBy=multi-user.target\n"
    )
}

/// The daemon's config file: its text and its mode. It holds only what the user passed to
/// setup. A file with an API key is `0600`, one without is `0644`.
#[must_use]
pub fn config_file(moonraker_url: Option<&str>, api_key: Option<&str>) -> (String, u32) {
    let quote = |text: &str| toml::Value::String(text.to_owned()).to_string();
    let mut text = String::from(
        "# Wobblescope's settings. Every key is optional; see the project's documentation.\n",
    );
    if let Some(url) = moonraker_url {
        text.push_str(&format!("moonraker_url = {}\n", quote(url)));
    }
    if let Some(key) = api_key {
        text.push_str(&format!("moonraker_api_key = {}\n", quote(key)));
    }
    (text, if api_key.is_some() { 0o600 } else { 0o644 })
}

/// `asvc` with `name` appended, or `None` when it is already listed.
#[must_use]
pub fn with_service(asvc: &str, name: &str) -> Option<String> {
    if asvc.lines().any(|line| line.trim() == name) {
        return None;
    }
    let mut updated = asvc.to_owned();
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(name);
    updated.push('\n');
    Some(updated)
}

/// `asvc` without `name`, or `None` when it is not listed.
#[must_use]
pub fn without_service(asvc: &str, name: &str) -> Option<String> {
    if !asvc.lines().any(|line| line.trim() == name) {
        return None;
    }
    Some(
        asvc.lines()
            .filter(|line| line.trim() != name)
            .map(|line| format!("{line}\n"))
            .collect(),
    )
}
