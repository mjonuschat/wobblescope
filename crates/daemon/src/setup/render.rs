//! The files setup owns, as text: the service unit, the release file Moonraker's updater reads,
//! the daemon's config, the block in `moonraker.conf` and the service list. Nothing here does
//! I/O.

use std::path::Path;

/// The unit's name, the service-list entry and the update-manager section's suffix.
pub const SERVICE: &str = "wobblescope";
/// The GitHub owner of the project.
pub const PROJECT_OWNER: &str = "mjonuschat";
/// The GitHub repository of the project.
pub const PROJECT_NAME: &str = "wobblescope";
/// The first line of the block setup adds to `moonraker.conf`.
pub const BLOCK_BEGIN: &str = "# BEGIN wobblescope (managed by `wobblescope setup`)";
/// The last line of that block.
pub const BLOCK_END: &str = "# END wobblescope";

/// The release asset built for CPU architecture `arch` (Rust's `std::env::consts::ARCH`), or
/// `None` when no asset is published for it. A 32-bit ARM binary also runs on 64-bit ARM, so
/// `arm` and `armv7` share a name.
#[must_use]
pub fn asset_for_arch(arch: &str) -> Option<&'static str> {
    match arch {
        "aarch64" => Some("wobblescope-aarch64"),
        "arm" | "armv7" => Some("wobblescope-armv7"),
        "x86_64" => Some("wobblescope-x86_64"),
        _ => None,
    }
}

/// The asset name of the running binary.
#[must_use]
pub fn asset_name() -> Option<&'static str> {
    asset_for_arch(std::env::consts::ARCH)
}

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

/// The `release_info.json` Moonraker's `executable` updater reads: the project, the asset to
/// download and the installed version as a release tag.
#[must_use]
pub fn release_info(asset: &str, crate_version: &str) -> String {
    let info = serde_json::json!({
        "project_owner": PROJECT_OWNER,
        "project_name": PROJECT_NAME,
        "asset_name": asset,
        "version": format!("v{crate_version}"),
    });
    let mut text = serde_json::to_string_pretty(&info).unwrap_or_default();
    text.push('\n');
    text
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

/// The marked `update_manager` block for `moonraker.conf`, ending in a newline.
#[must_use]
pub fn update_block(install_dir: &Path) -> String {
    format!(
        "{BLOCK_BEGIN}\n[update_manager {SERVICE}]\ntype: executable\nchannel: stable\n\
         repo: {PROJECT_OWNER}/{PROJECT_NAME}\npath: {}\nmanaged_services: {SERVICE}\n{BLOCK_END}\n",
        install_dir.display()
    )
}

/// Where our block starts and ends in `text`: the start of the begin line and the end of the
/// end line, newline included.
fn block_span(text: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut begin = None;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if begin.is_none() && trimmed == BLOCK_BEGIN {
            begin = Some(offset);
        } else if begin.is_some() && trimmed == BLOCK_END {
            return Some((begin?, offset + line.len()));
        }
        offset += line.len();
    }
    None
}

/// Whether `text` holds a complete marked block.
#[must_use]
pub fn has_block(text: &str) -> bool {
    block_span(text).is_some()
}

/// `conf` with `block` added, or replacing the marked block already there. `None` when nothing
/// would change.
#[must_use]
pub fn with_block(conf: &str, block: &str) -> Option<String> {
    if let Some((start, end)) = block_span(conf) {
        if &conf[start..end] == block {
            return None;
        }
        return Some(format!("{}{block}{}", &conf[..start], &conf[end..]));
    }
    let mut updated = conf.to_owned();
    if !updated.is_empty() {
        if !updated.ends_with('\n') {
            updated.push('\n');
        }
        updated.push('\n');
    }
    updated.push_str(block);
    Some(updated)
}

/// `conf` without the marked block and the blank line setup put before it, or `None` when it
/// has no block.
#[must_use]
pub fn without_block(conf: &str) -> Option<String> {
    let (start, end) = block_span(conf)?;
    let before = &conf[..start];
    let before = before.strip_suffix('\n').unwrap_or(before);
    let before = if start > 0 && conf[..start].ends_with("\n\n") {
        before
    } else {
        &conf[..start]
    };
    Some(format!("{before}{}", &conf[end..]))
}

/// Whether `conf` holds an `[update_manager wobblescope]` header outside our block.
#[must_use]
pub fn has_own_section(conf: &str) -> bool {
    let without = without_block(conf);
    let text = without.as_deref().unwrap_or(conf);
    let header = format!("[update_manager {SERVICE}]");
    text.lines().any(|line| line.trim() == header)
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
