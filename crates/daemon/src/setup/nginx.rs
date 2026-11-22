//! Reads the nginx configuration far enough to find the `server` blocks that proxy Moonraker.
//!
//! It understands comments, quoted words, `include` with `*` and `?` globs, `upstream` blocks
//! and `server`, `location`, `proxy_pass`, `listen` and `root` inside a server. It is not a
//! general nginx parser. A file it cannot tokenise or balance is skipped and reported.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// The nginx directory the paths in `nginx.conf` refer to.
pub const NGINX_DIR: &str = "/etc/nginx";

/// Suffix of the backup setup keeps beside a file it edits. Includes of that name are not
/// followed, so a backup in a globbed directory is never read as a second site.
pub const BACKUP_SUFFIX: &str = ".before-wobblescope";

/// One directive: its name, arguments and, for a block, the block's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Node {
    name: String,
    args: Vec<String>,
    start: usize,
    body: Option<Body>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Body {
    open: usize,
    close: usize,
    children: Vec<Node>,
}

#[derive(Debug, PartialEq, Eq)]
enum Tok {
    Word(String),
    Semi,
    Open,
    Close,
}

fn tokenize(text: &str) -> Result<Vec<(Tok, usize)>, String> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b';' => {
                tokens.push((Tok::Semi, i));
                i += 1;
            }
            b'{' => {
                tokens.push((Tok::Open, i));
                i += 1;
            }
            b'}' => {
                tokens.push((Tok::Close, i));
                i += 1;
            }
            _ => {
                let start = i;
                let mut word = Vec::new();
                while i < bytes.len() {
                    let c = bytes[i];
                    if matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b';' | b'{' | b'}') {
                        break;
                    }
                    if c == b'"' || c == b'\'' {
                        i += 1;
                        loop {
                            match bytes.get(i) {
                                None => return Err("a quoted string is not closed".to_owned()),
                                Some(b'\\') if i + 1 < bytes.len() => {
                                    word.push(bytes[i + 1]);
                                    i += 2;
                                }
                                Some(&q) if q == c => {
                                    i += 1;
                                    break;
                                }
                                Some(&other) => {
                                    word.push(other);
                                    i += 1;
                                }
                            }
                        }
                    } else {
                        word.push(c);
                        i += 1;
                    }
                }
                tokens.push((
                    Tok::Word(String::from_utf8_lossy(&word).into_owned()),
                    start,
                ));
            }
        }
    }
    Ok(tokens)
}

fn parse_nodes(tokens: &[(Tok, usize)], at: &mut usize, nested: bool) -> Result<Vec<Node>, String> {
    let mut nodes = Vec::new();
    while *at < tokens.len() {
        match &tokens[*at].0 {
            Tok::Close => {
                return if nested {
                    Ok(nodes)
                } else {
                    Err("a } has no matching {".to_owned())
                };
            }
            Tok::Semi => *at += 1,
            Tok::Open => return Err("a { has no directive name".to_owned()),
            Tok::Word(name) => {
                let start = tokens[*at].1;
                let mut args = Vec::new();
                *at += 1;
                loop {
                    match tokens.get(*at) {
                        None => return Err(format!("the directive {name} is not finished")),
                        Some((Tok::Word(arg), _)) => {
                            args.push(arg.clone());
                            *at += 1;
                        }
                        Some((Tok::Semi, _)) => {
                            *at += 1;
                            nodes.push(Node {
                                name: name.clone(),
                                args,
                                start,
                                body: None,
                            });
                            break;
                        }
                        Some((Tok::Open, open)) => {
                            let open = *open;
                            *at += 1;
                            let children = parse_nodes(tokens, at, true)?;
                            let Some((Tok::Close, close)) = tokens.get(*at) else {
                                return Err(format!("the block {name} is not closed"));
                            };
                            let close = *close;
                            *at += 1;
                            nodes.push(Node {
                                name: name.clone(),
                                args,
                                start,
                                body: Some(Body {
                                    open,
                                    close,
                                    children,
                                }),
                            });
                            break;
                        }
                        Some((Tok::Close, _)) => {
                            return Err(format!("the directive {name} is not finished"));
                        }
                    }
                }
            }
        }
    }
    if nested {
        Err("a { is not closed".to_owned())
    } else {
        Ok(nodes)
    }
}

fn parse(text: &str) -> Result<Vec<Node>, String> {
    let tokens = tokenize(text)?;
    parse_nodes(&tokens, &mut 0, false)
}

/// A `server` block, as far as setup needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// The file the block is in, as nginx reached it (possibly a symlink).
    pub file: PathBuf,
    /// Offset of the `{` that opens the block.
    pub open: usize,
    /// Offset of the `}` that closes the block.
    pub close: usize,
    /// Offset of the first `location` directive directly inside the block.
    pub first_location: Option<usize>,
    /// The first `listen` argument.
    pub listen: Option<String>,
    /// The `root` directive.
    pub root: Option<String>,
    /// The proxy targets (`apiserver`, `127.0.0.1:7125`) of the locations that look like
    /// Moonraker's.
    moonraker_targets: Vec<String>,
}

impl Server {
    /// A name for messages: the file name and the listen argument.
    #[must_use]
    pub fn label(&self) -> String {
        let name = self
            .file
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match &self.listen {
            Some(listen) => format!("{name} (listen {listen})"),
            None => name,
        }
    }

    /// Whether the root directory is a Mainsail install.
    #[must_use]
    pub fn is_mainsail(&self) -> bool {
        self.root
            .as_deref()
            .is_some_and(|r| r.trim_end_matches('/').ends_with("mainsail"))
    }
}

/// The upstream or literal address a `proxy_pass` URL names: `http://apiserver/x` gives
/// `apiserver`, `http://127.0.0.1:7125$request_uri` gives `127.0.0.1:7125`.
fn proxy_target(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let end = rest.find(['/', '$']).unwrap_or(rest.len());
    let target = &rest[..end];
    (!target.is_empty()).then(|| target.to_owned())
}

fn is_moonraker_location(args: &[String]) -> bool {
    match args {
        [path] => path == "/websocket",
        [modifier, path] if matches!(modifier.as_str(), "=" | "^~") => path == "/websocket",
        [modifier, regex] if matches!(modifier.as_str(), "~" | "~*") => {
            regex.starts_with("^/(") && regex.contains("printer") && regex.contains("api")
        }
        _ => false,
    }
}

fn read_server(file: &Path, node: &Node) -> Option<Server> {
    let body = node.body.as_ref()?;
    let mut server = Server {
        file: file.to_owned(),
        open: body.open,
        close: body.close,
        first_location: None,
        listen: None,
        root: None,
        moonraker_targets: Vec::new(),
    };
    for child in &body.children {
        match child.name.as_str() {
            "listen" if server.listen.is_none() => server.listen = child.args.first().cloned(),
            "root" if server.root.is_none() => server.root = child.args.first().cloned(),
            "location" => {
                server.first_location.get_or_insert(child.start);
                if let Some(inner) = &child.body
                    && is_moonraker_location(&child.args)
                {
                    let target = inner
                        .children
                        .iter()
                        .find(|n| n.name == "proxy_pass")
                        .and_then(|n| n.args.first())
                        .and_then(|url| proxy_target(url));
                    server.moonraker_targets.extend(target);
                }
            }
            _ => {}
        }
    }
    Some(server)
}

/// What reading the nginx configuration found.
#[derive(Debug, Default)]
pub struct Layout {
    /// Every file reached, including those that could not be understood.
    pub files: Vec<PathBuf>,
    /// Files that were skipped, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
    /// Every `server` block in the `http` context.
    pub servers: Vec<Server>,
    upstreams: BTreeMap<String, String>,
}

/// The servers that proxy Moonraker, split by whether they proxy the instance setup targets.
#[derive(Debug, Default)]
pub struct Plan {
    /// Servers whose Moonraker port is the one asked for.
    pub mapped: Vec<Server>,
    /// Servers that proxy Moonraker somewhere else, with why they are left alone.
    pub left_alone: Vec<(Server, String)>,
}

/// Whether `name` matches `pattern`, where `*` is any run of characters and `?` one.
fn wild(pattern: &[u8], name: &[u8]) -> bool {
    match pattern.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|n| wild(rest, &name[n..])),
        Some((b'?', rest)) => !name.is_empty() && wild(rest, &name[1..]),
        Some((c, rest)) => name.first() == Some(c) && wild(rest, &name[1..]),
    }
}

/// The existing files `pattern` names, in name order.
fn expand(pattern: &Path) -> Vec<PathBuf> {
    let mut found = vec![PathBuf::new()];
    for part in pattern.components() {
        let part_text = part.as_os_str().to_string_lossy();
        let mut next = Vec::new();
        if part_text.contains(['*', '?']) {
            for base in &found {
                let Ok(entries) = fs::read_dir(base) else {
                    continue;
                };
                let mut names: Vec<String> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.starts_with('.') && wild(part_text.as_bytes(), n.as_bytes()))
                    .collect();
                names.sort();
                next.extend(names.into_iter().map(|n| base.join(n)));
            }
        } else {
            next.extend(found.iter().map(|base| base.join(part)));
        }
        found = next;
    }
    found.retain(|p| p.is_file());
    found
}

/// Maps a path from `nginx.conf` into `dir`: `/etc/nginx/x` and relative `x` become `dir/x`.
/// Any other absolute path is not followed.
fn locate(dir: &Path, text: &str) -> Option<PathBuf> {
    let path = Path::new(text);
    if path.is_absolute() {
        path.strip_prefix(NGINX_DIR).ok().map(|rest| dir.join(rest))
    } else {
        Some(dir.join(path))
    }
}

struct Reader<'a> {
    dir: &'a Path,
    layout: Layout,
    seen: BTreeSet<PathBuf>,
}

impl Reader<'_> {
    fn file(&mut self, path: &Path, depth: usize) {
        let key = fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
        if depth > 8 || !self.seen.insert(key) {
            return;
        }
        self.layout.files.push(path.to_owned());
        let nodes = fs::read_to_string(path)
            .map_err(|e| format!("cannot read it: {e}"))
            .and_then(|text| parse(&text).map_err(|e| format!("not understood: {e}")));
        match nodes {
            Ok(nodes) => self.nodes(path, &nodes, depth),
            Err(reason) => self.layout.skipped.push((path.to_owned(), reason)),
        }
    }

    fn nodes(&mut self, file: &Path, nodes: &[Node], depth: usize) {
        for node in nodes {
            match (node.name.as_str(), &node.body) {
                ("include", None) => {
                    for pattern in &node.args {
                        let Some(pattern) = locate(self.dir, pattern) else {
                            continue;
                        };
                        for path in expand(&pattern) {
                            if !path.to_string_lossy().ends_with(BACKUP_SUFFIX) {
                                self.file(&path, depth + 1);
                            }
                        }
                    }
                }
                ("upstream", Some(body)) => {
                    let first = body.children.iter().find(|n| n.name == "server");
                    if let (Some(name), Some(server)) =
                        (node.args.first(), first.and_then(|n| n.args.first()))
                    {
                        self.layout
                            .upstreams
                            .entry(name.clone())
                            .or_insert_with(|| server.clone());
                    }
                }
                ("server", Some(_)) => self.layout.servers.extend(read_server(file, node)),
                ("mail" | "stream" | "events", _) => {}
                (_, Some(body)) => self.nodes(file, &body.children, depth),
                _ => {}
            }
        }
    }
}

/// The port of `host:port`, `[::1]:port`, or the upstream `name`'s first server.
fn port_of(target: &str, upstreams: &BTreeMap<String, String>) -> Result<u16, String> {
    let address = upstreams.get(target).map_or(target, String::as_str);
    if address.starts_with("unix:") {
        return Err(format!("{target} is a unix socket"));
    }
    address
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse().ok())
        .ok_or_else(|| format!("{target} does not resolve to a host and port"))
}

impl Layout {
    /// Reads `dir/nginx.conf` and what it includes. `None` when there is no `nginx.conf`.
    #[must_use]
    pub fn read(dir: &Path) -> Option<Self> {
        let main = dir.join("nginx.conf");
        if !main.is_file() {
            return None;
        }
        let mut reader = Reader {
            dir,
            layout: Layout::default(),
            seen: BTreeSet::new(),
        };
        reader.file(&main, 0);
        Some(reader.layout)
    }

    /// Sorts the servers that proxy Moonraker by whether they proxy the instance at `port`.
    /// Servers that do not proxy Moonraker are not listed at all.
    #[must_use]
    pub fn plan(&self, port: u16) -> Plan {
        let mut plan = Plan::default();
        for server in &self.servers {
            if server.moonraker_targets.is_empty() {
                continue;
            }
            let ports: Vec<Result<u16, String>> = server
                .moonraker_targets
                .iter()
                .map(|t| port_of(t, &self.upstreams))
                .collect();
            match ports.iter().find_map(|p| p.as_ref().err()) {
                Some(why) => plan.left_alone.push((server.clone(), why.clone())),
                None if ports.iter().all(|p| *p == Ok(port)) => plan.mapped.push(server.clone()),
                None => {
                    let other = ports.iter().flatten().find(|p| **p != port);
                    plan.left_alone.push((
                        server.clone(),
                        format!(
                            "it proxies Moonraker on port {}, not {port}",
                            other.map_or_else(String::new, ToString::to_string)
                        ),
                    ));
                }
            }
        }
        plan
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_quotes_and_blocks_parse() {
        let nodes = parse("a b; # note\nserver { listen \"81\"; x 'y;z'; }\n").unwrap();
        assert_eq!(nodes.len(), 2);
        let body = nodes[1].body.as_ref().unwrap();
        assert_eq!(body.children[0].args, ["81"]);
        assert_eq!(body.children[1].args, ["y;z"]);
    }

    #[test]
    fn unbalanced_files_are_refused() {
        assert!(parse("server { listen 80;").is_err());
        assert!(parse("}").is_err());
        assert!(parse("a \"b;").is_err());
        assert!(parse("listen 80").is_err());
    }

    #[test]
    fn globs_match_stars_and_marks() {
        assert!(wild(b"*.conf", b"a.conf"));
        assert!(!wild(b"*.conf", b"a.confx"));
        assert!(wild(b"a?c", b"abc"));
        assert!(wild(b"*", b""));
    }

    #[test]
    fn proxy_targets_drop_the_scheme_path_and_variables() {
        assert_eq!(
            proxy_target("http://apiserver/websocket").as_deref(),
            Some("apiserver")
        );
        assert_eq!(
            proxy_target("http://127.0.0.1:7125$request_uri").as_deref(),
            Some("127.0.0.1:7125")
        );
        assert_eq!(proxy_target("apiserver"), None);
    }
}
