//! The files setup owns, as text: the service unit, the daemon's config and the service list.
//! Nothing here does I/O.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
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

/// First line of the nginx snippet. A snippet without it is not ours and is never deleted.
pub const SNIPPET_HEADER: &str = "# managed by wobblescope setup";

/// Where nginx finds the snippet. The path is the one in `nginx.conf` terms, not the test
/// directory.
pub const SNIPPET_INCLUDE_PATH: &str = "/etc/nginx/snippets/wobblescope.conf";

const BEGIN_MARK: &str = "# BEGIN wobblescope";
const END_MARK: &str = "# END wobblescope";

const FORWARDED: &str = "    proxy_set_header Host $http_host;\n\
\x20   proxy_set_header X-Real-IP $remote_addr;\n\
\x20   proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;\n\
\x20   proxy_set_header X-Forwarded-Proto $scheme;\n";

/// The nginx snippet that serves the daemon listening on `port` under `/wobblescope/`.
/// It holds only `location` blocks, so it can sit in any `server` block.
#[must_use]
pub fn snippet(port: u16) -> String {
    let mut text = format!(
        "{SNIPPET_HEADER}\n\
         # Serves Wobblescope under /wobblescope/. Remove it with: sudo wobblescope setup --remove\n\n\
         location = /wobblescope {{ return 301 /wobblescope/; }}\n"
    );
    for path in ["live", "stream", "hold"] {
        text.push_str(&format!(
            "\nlocation ^~ /wobblescope/api/{path} {{\n\
             \x20   proxy_pass http://127.0.0.1:{port}/api/{path};\n\
             \x20   proxy_http_version 1.1;\n\
             \x20   proxy_set_header Upgrade $http_upgrade;\n\
             \x20   proxy_set_header Connection \"upgrade\";\n\
             {FORWARDED}\
             \x20   proxy_read_timeout 1d;\n\
             }}\n"
        ));
    }
    text.push_str(&format!(
        "\nlocation ^~ /wobblescope/ {{\n\
         \x20   proxy_pass http://127.0.0.1:{port}/;\n\
         \x20   proxy_http_version 1.1;\n\
         {FORWARDED}\
         \x20   proxy_read_timeout 1h;\n\
         \x20   proxy_send_timeout 1h;\n\
         \x20   client_max_body_size 64m;\n\
         }}\n"
    ));
    text
}

fn block(indent: &str) -> String {
    format!("{indent}{BEGIN_MARK}\n{indent}include {SNIPPET_INCLUDE_PATH};\n{indent}{END_MARK}\n")
}

fn marked_blocks(text: &str) -> Result<Vec<(usize, usize)>, String> {
    let mut found = Vec::new();
    let mut begin = None;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        match (line.trim(), begin) {
            (BEGIN_MARK, None) => begin = Some(at),
            (END_MARK, Some(start)) => {
                found.push((start, at + line.len()));
                begin = None;
            }
            (BEGIN_MARK, Some(_)) => return Err("a BEGIN wobblescope marker is repeated".into()),
            _ => {}
        }
        at += line.len();
    }
    if begin.is_some() {
        return Err("a BEGIN wobblescope marker has no END".into());
    }
    Ok(found)
}

/// `text` with the include block inside the `server` block whose braces are at `open` and
/// `close`, placed before the block's first `location` (at offset `first_location`) with that
/// line's indentation. A block already there is replaced when it differs. `None` when the text
/// is already right.
///
/// # Errors
/// A message when the offsets do not fit `text` or the markers are unbalanced.
pub fn with_include_block(
    text: &str,
    open: usize,
    close: usize,
    first_location: usize,
) -> Result<Option<String>, String> {
    if text.get(open..=open) != Some("{") || text.get(close..=close) != Some("}") {
        return Err("the file changed while it was being read".into());
    }
    let body = &text[open..close];
    if let Some(&(start, end)) = marked_blocks(body)?.first() {
        let (start, end) = (open + start, open + end);
        let line = &text[start..end];
        let indent = &line[..line.len() - line.trim_start().len()];
        let wanted = block(indent);
        if text[start..end] == wanted {
            return Ok(None);
        }
        return Ok(Some(format!("{}{wanted}{}", &text[..start], &text[end..])));
    }
    let line_start = text[..first_location].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..first_location];
    let indent = &line[..line.len() - line.trim_start().len()];
    Ok(Some(format!(
        "{}{}{}",
        &text[..line_start],
        block(indent),
        &text[line_start..]
    )))
}

pub fn without_include_blocks(text: &str) -> Result<Option<String>, String> {
    let blocks = marked_blocks(text)?;
    if blocks.is_empty() {
        return Ok(None);
    }
    let mut kept = String::with_capacity(text.len());
    let mut at = 0;
    for (start, end) in blocks {
        kept.push_str(&text[at..start]);
        at = end;
    }
    kept.push_str(&text[at..]);
    Ok(Some(kept))
}

const LOOPBACK: [IpAddr; 2] = [
    IpAddr::V4(Ipv4Addr::LOCALHOST),
    IpAddr::V6(Ipv6Addr::LOCALHOST),
];

/// The config text with `trusted_proxies` listing both loopback addresses, or `None` when it
/// already does. A missing key is added before the first table header or at the end. An
/// existing array is rewritten on one line with the loopback addresses it lacks added. The
/// rest of the file is kept as written.
///
/// # Errors
/// A message when the text is not TOML or `trusted_proxies` is not a list of strings.
pub fn with_trusted_proxies(text: &str) -> Result<Option<String>, String> {
    use crate::access::policy::Net;
    let table: toml::Table = text.parse().map_err(|e| format!("not valid TOML: {e}"))?;
    fn quote(s: &str) -> String {
        toml::Value::String(s.to_owned()).to_string()
    }
    let Some(value) = table.get("trusted_proxies") else {
        let line = "trusted_proxies = [\"127.0.0.1\", \"::1\"]\n";
        let at = text
            .split_inclusive('\n')
            .scan(0, |at, l| {
                let start = *at;
                *at += l.len();
                Some((start, l))
            })
            .find(|(_, l)| l.trim_start().starts_with('['))
            .map_or(text.len(), |(start, _)| start);
        let mut out = text[..at].to_owned();
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(line);
        if at < text.len() {
            out.push('\n');
        }
        out.push_str(&text[at..]);
        return Ok(Some(out));
    };
    let entries: Vec<&str> = value
        .as_array()
        .and_then(|a| a.iter().map(toml::Value::as_str).collect())
        .ok_or("trusted_proxies is not a list of strings")?;
    let nets: Vec<Net> = entries.iter().filter_map(|e| Net::parse(e).ok()).collect();
    let missing: Vec<String> = LOOPBACK
        .iter()
        .filter(|ip| !nets.iter().any(|n| n.contains(**ip)))
        .map(ToString::to_string)
        .collect();
    if missing.is_empty() {
        return Ok(None);
    }
    let all: Vec<String> = entries
        .iter()
        .copied()
        .chain(missing.iter().map(String::as_str))
        .map(quote)
        .collect();
    let array = format!("[{}]", all.join(", "));
    let (start, end) =
        array_span(text).ok_or("cannot find the trusted_proxies list in the file")?;
    let out = format!("{}{array}{}", &text[..start], &text[end..]);
    let check: toml::Table = out
        .parse()
        .map_err(|e| format!("rewrite is not valid TOML: {e}"))?;
    if check
        .get("trusted_proxies")
        .and_then(toml::Value::as_array)
        .map(Vec::len)
        != Some(all.len())
    {
        return Err("cannot rewrite the trusted_proxies list safely".into());
    }
    Ok(Some(out))
}

fn array_span(text: &str) -> Option<(usize, usize)> {
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let rest = line.trim_start();
        if let Some(after) = rest.strip_prefix("trusted_proxies") {
            let after = after.trim_start();
            if let Some(value) = after.strip_prefix('=') {
                let value = value.trim_start();
                if value.starts_with('[') {
                    let start = at + (line.len() - value.len());
                    return array_end(text, start).map(|end| (start, end));
                }
            }
        }
        at += line.len();
    }
    None
}

fn array_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let (mut i, mut depth) = (start, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < bytes.len() && bytes[i] != q {
                    if q == b'"' && bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
