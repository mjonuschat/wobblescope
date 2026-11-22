//! The Mainsail sidebar entry: one object in `<config>/.theme/navi.json`.
//!
//! The file is the user's. An entry is added as text before the closing `]` so the rest of
//! the file keeps its formatting. Our entry is the one whose `title` is "Wobblescope" and whose
//! `href` is `/wobblescope/`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::SetupError;
use super::host::{Host, write_atomic};
use super::install::{Say, io_err, read};
use super::nginx_link::back_up;
use crate::setup::nginx::BACKUP_SUFFIX;

const TITLE: &str = "Wobblescope";
const HREF: &str = "/wobblescope/";

/// The icon is `mdiChartBellCurve` from Material Design Icons, Apache License 2.0, taken from
/// `mdi.js` in https://github.com/Templarian/MaterialDesign-JS (also `@mdi/js`).
const ICON: &str = "M9.96,11.31C10.82,8.1 11.5,6 13,6C14.5,6 15.18,8.1 16.04,11.31C17,14.92 18.1,19 22,19V17C19.8,17 19,14.54 17.97,10.8C17.08,7.46 16.15,4 13,4C9.85,4 8.92,7.46 8.03,10.8C7.03,14.54 6.2,17 4,17V2H2V22H22V20H4V19C7.9,19 9,14.92 9.96,11.31Z";

/// Mainsail numbers its own entries 10 to 90 in tens, and Machine is last at 90, so 95 puts the
/// link after it.
const POSITION: u32 = 95;

/// Our entry as a JSON value.
#[must_use]
pub fn entry() -> Value {
    json!({
        "title": TITLE,
        "href": HREF,
        "target": "_self",
        "position": POSITION,
        "icon": ICON,
    })
}

fn fields() -> Vec<(&'static str, String)> {
    let value = entry();
    ["title", "href", "target", "position", "icon"]
        .into_iter()
        .map(|key| (key, value[key].to_string()))
        .collect()
}

fn compact() -> String {
    let body: Vec<String> = fields()
        .iter()
        .map(|(key, value)| format!("\"{key}\": {value}"))
        .collect();
    format!("{{{}}}", body.join(", "))
}

/// The entry over several lines: `indent` before the braces, `field_indent` before each key.
fn spread(indent: &str, field_indent: &str) -> String {
    let body: Vec<String> = fields()
        .iter()
        .map(|(key, value)| format!("{field_indent}\"{key}\": {value}"))
        .collect();
    format!("{{\n{}\n{indent}}}", body.join(",\n"))
}

fn is_ours(value: &Value) -> bool {
    value["title"] == TITLE && value["href"] == HREF
}

fn skip_string(bytes: &[u8], mut at: usize) -> Option<usize> {
    at += 1;
    while at < bytes.len() {
        match bytes[at] {
            b'\\' => at += 2,
            b'"' => return Some(at + 1),
            _ => at += 1,
        }
    }
    None
}

fn skip_value(bytes: &[u8], start: usize) -> Option<usize> {
    let mut at = start;
    match bytes.get(at)? {
        b'"' => skip_string(bytes, at),
        b'{' | b'[' => {
            let mut depth = 0;
            while at < bytes.len() {
                match bytes[at] {
                    b'"' => {
                        at = skip_string(bytes, at)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(at + 1);
                        }
                    }
                    _ => {}
                }
                at += 1;
            }
            None
        }
        _ => {
            while at < bytes.len()
                && !matches!(bytes[at], b',' | b']' | b' ' | b'\t' | b'\r' | b'\n')
            {
                at += 1;
            }
            Some(at)
        }
    }
}

/// The byte ranges of the elements of the top-level array in `text`, which must already be
/// valid JSON.
fn elements(text: &str) -> Option<Vec<(usize, usize)>> {
    let bytes = text.as_bytes();
    let mut at = text.find('[')? + 1;
    let mut spans = Vec::new();
    loop {
        while at < bytes.len() && matches!(bytes[at], b',' | b' ' | b'\t' | b'\r' | b'\n') {
            at += 1;
        }
        if *bytes.get(at)? == b']' {
            return Some(spans);
        }
        let end = skip_value(bytes, at)?;
        spans.push((at, end));
        at = end;
    }
}

/// The leading whitespace of the line `start` is on, when only whitespace precedes it there.
fn own_line_indent(text: &str, start: usize) -> Option<&str> {
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let before = &text[line_start..start];
    before.trim().is_empty().then_some(before)
}

/// Our entry written in the style of the element at `span`: compact when that element is on
/// one line, spread with its field indentation otherwise.
fn styled(text: &str, span: (usize, usize)) -> String {
    let element = &text[span.0..span.1];
    if !element.contains('\n') {
        return compact();
    }
    let indent = own_line_indent(text, span.0).unwrap_or("");
    let field_indent = element
        .lines()
        .nth(1)
        .map(|l| &l[..l.len() - l.trim_start().len()])
        .filter(|f| !f.is_empty())
        .map_or_else(|| format!("{indent}  "), str::to_owned);
    spread(indent, &field_indent)
}

/// A new file holding only our entry.
#[must_use]
pub fn new_file() -> String {
    format!("[\n  {}\n]\n", spread("  ", "    "))
}

/// What adding our entry to a file's text does.
#[derive(Debug, PartialEq, Eq)]
pub enum Merge {
    /// Our entry is there and equal.
    Unchanged,
    /// The text to write.
    Changed(String),
}

fn parse_array(text: &str) -> Result<Vec<Value>, String> {
    match serde_json::from_str::<Value>(text) {
        Ok(Value::Array(items)) => Ok(items),
        Ok(_) => Err("it is not a JSON array".to_owned()),
        Err(e) => Err(format!("it is not valid JSON: {e}")),
    }
}

/// `text` with our entry added or replaced, leaving every other byte as it was.
///
/// # Errors
/// A message when `text` is not a JSON array or the edit would not parse.
pub fn with_entry(text: &str) -> Result<Merge, String> {
    let items = parse_array(text)?;
    if items.is_empty() {
        return Ok(Merge::Changed(new_file()));
    }
    let spans = elements(text).ok_or("cannot find the end of the array")?;
    let wanted = entry();
    let ours = spans
        .iter()
        .zip(&items)
        .find(|(_, item)| is_ours(item))
        .map(|(span, item)| (*span, item));
    let edited = match ours {
        Some((_, item)) if *item == wanted => return Ok(Merge::Unchanged),
        Some((span, _)) => format!(
            "{}{}{}",
            &text[..span.0],
            styled(text, span),
            &text[span.1..]
        ),
        None => {
            let last = *spans.last().ok_or("cannot find the last entry")?;
            let separator = own_line_indent(text, last.0)
                .map_or_else(|| ", ".to_owned(), |indent| format!(",\n{indent}"));
            format!(
                "{}{separator}{}{}",
                &text[..last.1],
                styled(text, last),
                &text[last.1..]
            )
        }
    };
    let after =
        parse_array(&edited).map_err(|e| format!("the edit would leave the file invalid ({e})"))?;
    if after.iter().filter(|i| **i == wanted).count() != 1 {
        return Err("the edit did not produce exactly one Wobblescope entry".to_owned());
    }
    Ok(Merge::Changed(edited))
}

/// What removing our entry from a file's text leaves.
#[derive(Debug, PartialEq, Eq)]
pub enum Strip {
    /// Our entry is not there.
    Absent,
    /// The file's text without our entry, and whether the array is now empty.
    Removed { text: String, empty: bool },
}

/// `text` without our entry, re-serialised.
///
/// # Errors
/// A message when `text` is not a JSON array.
pub fn without_entry(text: &str) -> Result<Strip, String> {
    let items = parse_array(text)?;
    if !items.iter().any(is_ours) {
        return Ok(Strip::Absent);
    }
    let rest: Vec<Value> = items.into_iter().filter(|i| !is_ours(i)).collect();
    let mut text = serde_json::to_string_pretty(&rest).map_err(|e| e.to_string())?;
    text.push('\n');
    Ok(Strip::Removed {
        empty: rest.is_empty(),
        text,
    })
}

/// Where the file is for a Moonraker config directory.
#[must_use]
pub fn path(config_root: &Path) -> PathBuf {
    config_root.join(".theme/navi.json")
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(BACKUP_SUFFIX);
    PathBuf::from(name)
}

/// Adds or updates our entry. A file that is not a JSON array is refused with a message and
/// left alone; that is reported, not raised.
///
/// # Errors
/// [`SetupError`] when the file cannot be read or written.
pub(super) fn install(host: &Host, say: &mut Say, config_root: &Path) -> Result<(), SetupError> {
    let file = path(config_root);
    let existing = read(&file)?;
    let merged = match &existing {
        None => Ok(Merge::Changed(new_file())),
        Some(text) => with_entry(text),
    };
    match merged {
        Err(why) => {
            say.step("mainsail entry", "skipped, nothing written");
            say.note(&format!("{}: {why}", file.display()));
        }
        Ok(Merge::Unchanged) => say.step("mainsail entry", "already current"),
        Ok(Merge::Changed(text)) => {
            if let Some(dir) = file.parent()
                && !dir.exists()
            {
                fs::create_dir(dir).map_err(io_err(format!("cannot create {}", dir.display())))?;
                if let Some((uid, gid)) = host.owner() {
                    std::os::unix::fs::chown(dir, Some(uid), Some(gid))
                        .map_err(io_err(format!("cannot chown {}", dir.display())))?;
                }
            }
            let (mode, owner) = match &existing {
                Some(old) => {
                    back_up(&fs::canonicalize(&file).unwrap_or(file.clone()), old)?;
                    (None, None)
                }
                None => (Some(0o644), host.owner()),
            };
            write_atomic(&file, text.as_bytes(), mode, owner)
                .map_err(io_err(format!("cannot write {}", file.display())))?;
            say.step(
                "mainsail entry",
                if existing.is_some() {
                    "updated"
                } else {
                    "installed"
                },
            );
        }
    }
    Ok(())
}

/// Whether our entry is in the file and equal. Prints the state.
///
/// # Errors
/// [`SetupError`] when the file cannot be read.
pub(super) fn check(say: &mut Say, config_root: &Path) -> Result<bool, SetupError> {
    let file = path(config_root);
    let (good, state) = match read(&file)? {
        None => (false, "missing".to_owned()),
        Some(text) => match with_entry(&text) {
            Ok(Merge::Unchanged) => (true, "ok".to_owned()),
            Ok(Merge::Changed(_)) => (false, "missing or different".to_owned()),
            Err(why) => (false, format!("cannot be read: {why}")),
        },
    };
    say.step("mainsail entry", &state);
    Ok(good)
}

/// Takes our entry out. The file goes too when it then holds an empty array and no backup
/// exists, which means setup created it.
///
/// # Errors
/// [`SetupError`] when the file cannot be read or written.
pub(super) fn remove(say: &mut Say, config_root: &Path) -> Result<(), SetupError> {
    let file = path(config_root);
    let Some(text) = read(&file)? else {
        return Ok(());
    };
    match without_entry(&text) {
        Err(why) => say.note(&format!(
            "mainsail entry: {}: {why}; left alone",
            file.display()
        )),
        Ok(Strip::Absent) => say.step("mainsail entry", "no entry"),
        Ok(Strip::Removed { text, empty }) => {
            let target = fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
            if empty && !backup_path(&target).exists() {
                fs::remove_file(&target)
                    .map_err(io_err(format!("cannot remove {}", target.display())))?;
                say.step("mainsail entry", "removed with the file setup created");
            } else {
                write_atomic(&file, text.as_bytes(), None, None)
                    .map_err(io_err(format!("cannot write {}", file.display())))?;
                say.step("mainsail entry", "removed");
            }
        }
    }
    Ok(())
}
