//! The pure parts of the guard: who the client is, and whether the `Host` and `Origin` of a
//! request are acceptable. Nothing here knows about HTTP types.

use std::net::IpAddr;

use super::policy::{Net, Policy, normalize};

/// Who a request comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Client {
    /// The client's address, with an IPv4-mapped address turned into IPv4.
    pub ip: IpAddr,
    /// Whether the peer sent a forwarding header although it is not a configured proxy, so the
    /// headers were ignored.
    pub ignored_forwarded: bool,
}

/// The values of the headers a reverse proxy uses to name the client behind it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ForwardingHeaders<'a> {
    /// Every `Forwarded` header (RFC 7239).
    pub forwarded: &'a [&'a str],
    pub x_forwarded_for: &'a [&'a str],
    pub x_real_ip: &'a [&'a str],
}

impl ForwardingHeaders<'_> {
    fn is_empty(&self) -> bool {
        self.forwarded.is_empty() && self.x_forwarded_for.is_empty() && self.x_real_ip.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardingError {
    NotAnAddress(&'static str),
    Conflict,
}

impl std::fmt::Display for ForwardingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnAddress(header) => {
                write!(
                    formatter,
                    "{header} holds something that is not an IP address"
                )
            }
            Self::Conflict => formatter
                .write_str("Forwarded, X-Forwarded-For and X-Real-IP name different clients"),
        }
    }
}

/// Resolves the client from the TCP `peer` and the proxy `headers`.
///
/// The peer is the client unless it is in `proxies`. Then each header present names a client:
/// the rightmost entry of `Forwarded` or `X-Forwarded-For` that is not a proxy, or the leftmost
/// entry when all of them are, and the last address in `X-Real-IP`, which is the client even
/// when it lies in a proxy network. A client header that a proxy passes through unchanged could be forged, so when several headers are
/// present they must all name the same client.
///
/// # Errors
/// [`ForwardingError`] when the peer is a proxy and an entry is not an IP address, or the
/// headers disagree.
pub fn resolve_client(
    peer: IpAddr,
    headers: &ForwardingHeaders<'_>,
    proxies: &[Net],
) -> Result<Client, ForwardingError> {
    let peer = normalize(peer);
    let is_proxy = |ip: IpAddr| proxies.iter().any(|net| net.contains(ip));
    if !is_proxy(peer) {
        return Ok(Client {
            ip: peer,
            ignored_forwarded: !headers.is_empty(),
        });
    }
    let mut named = Vec::new();
    let mut chain = |header: &'static str, entries: Vec<IpAddr>| {
        let pick = entries
            .iter()
            .rev()
            .find(|ip| !is_proxy(**ip))
            .or(entries.first());
        if let Some(ip) = pick {
            named.push((header, *ip));
        }
    };
    chain("Forwarded", forwarded_entries(headers.forwarded)?);
    chain(
        "X-Forwarded-For",
        list_entries("X-Forwarded-For", headers.x_forwarded_for, true)?,
    );
    if let Some(ip) = list_entries("X-Real-IP", headers.x_real_ip, false)?.last() {
        named.push(("X-Real-IP", *ip));
    }
    let Some(&(_, ip)) = named.first() else {
        return Ok(Client {
            ip: peer,
            ignored_forwarded: false,
        });
    };
    if named.iter().any(|&(_, other)| other != ip) {
        return Err(ForwardingError::Conflict);
    }
    Ok(Client {
        ip,
        ignored_forwarded: false,
    })
}

fn list_entries(
    header: &'static str,
    values: &[&str],
    list: bool,
) -> Result<Vec<IpAddr>, ForwardingError> {
    let mut entries = Vec::new();
    for value in values {
        let parts: Vec<&str> = if list {
            value.split(',').collect()
        } else {
            vec![value]
        };
        for part in parts {
            let ip: IpAddr = part
                .trim()
                .parse()
                .map_err(|_| ForwardingError::NotAnAddress(header))?;
            entries.push(normalize(ip));
        }
    }
    Ok(entries)
}

fn forwarded_entries(values: &[&str]) -> Result<Vec<IpAddr>, ForwardingError> {
    let bad = || ForwardingError::NotAnAddress("Forwarded");
    let mut entries = Vec::new();
    for value in values {
        for element in split_outside_quotes(value, ',').ok_or_else(bad)? {
            for pair in split_outside_quotes(element, ';').ok_or_else(bad)? {
                let Some((name, node)) = pair.split_once('=') else {
                    continue;
                };
                if !name.trim().eq_ignore_ascii_case("for") {
                    continue;
                }
                entries.push(normalize(forwarded_node(node.trim()).ok_or_else(bad)?));
            }
        }
    }
    Ok(entries)
}

/// The IP address in a `for` value, which may be quoted, bracketed (IPv6) and followed by a
/// port. An obfuscated name or `unknown` has no address.
fn forwarded_node(node: &str) -> Option<IpAddr> {
    let node = node
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(node);
    if let Some(rest) = node.strip_prefix('[') {
        return rest.split_once(']')?.0.parse().ok();
    }
    if node.matches(':').count() == 1 {
        return node.split_once(':')?.0.parse().ok();
    }
    node.parse().ok()
}

/// Splits `text` at each `separator` outside double-quoted strings, where a backslash escapes the
/// next character. `None` when a quoted string is not closed: the rest of the value could be
/// something a proxy appended, which must not be read as part of a client-supplied string.
fn split_outside_quotes(text: &str, separator: char) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    for (index, c) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == separator && !quoted {
            parts.push(&text[start..index]);
            start = index + c.len_utf8();
        }
    }
    if quoted {
        return None;
    }
    parts.push(&text[start..]);
    Some(parts)
}

/// The host name in a `Host` value: lowercase, with the port and a trailing dot removed and an
/// IPv6 literal without its brackets. `None` for a value with no usable host.
#[must_use]
pub fn host_name(authority: &str) -> Option<String> {
    let host = if let Some(rest) = authority.strip_prefix('[') {
        rest.split_once(']')?.0
    } else if authority.matches(':').count() == 1 {
        authority.split_once(':')?.0
    } else {
        authority
    };
    let host = host.trim_end_matches('.').to_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Whether the host of a request is acceptable. `authority` is the `Host` value or the request
/// URI's authority. Without a known `policy`, only IP literals, `localhost` and
/// `allowed_hosts` count.
#[must_use]
pub fn host_allowed(authority: &str, policy: Option<&Policy>, allowed_hosts: &[String]) -> bool {
    let Some(host) = host_name(authority) else {
        return false;
    };
    host.parse::<IpAddr>().is_ok()
        || host == "localhost"
        || allowed_hosts.contains(&host)
        || policy.is_some_and(|p| p.host_allowed(&host))
}

/// Whether `origin` is the daemon's own: its authority equals the `Host` value, ignoring case.
#[must_use]
pub fn same_origin(origin: &str, authority: &str) -> bool {
    origin
        .split_once("://")
        .is_some_and(|(_, rest)| rest.eq_ignore_ascii_case(authority))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nets(texts: &[&str]) -> Vec<Net> {
        texts.iter().map(|t| Net::parse(t).unwrap()).collect()
    }

    fn resolve(headers: &ForwardingHeaders<'_>) -> Result<IpAddr, ForwardingError> {
        resolve_client(
            "10.0.0.1".parse().unwrap(),
            headers,
            &nets(&["10.0.0.0/24"]),
        )
        .map(|client| client.ip)
    }

    fn ip(text: &str) -> IpAddr {
        text.parse().unwrap()
    }

    #[test]
    fn a_forwarded_header_names_its_for_address() {
        let headers = ForwardingHeaders {
            forwarded: &["for=192.0.2.43;proto=https;by=10.0.0.1"],
            ..Default::default()
        };
        assert_eq!(resolve(&headers), Ok(ip("192.0.2.43")));
    }

    #[test]
    fn forwarded_for_values_may_be_quoted_bracketed_and_carry_a_port() {
        for (value, want) in [
            ("for=\"192.0.2.43:47011\"", "192.0.2.43"),
            ("for=\"[2001:db8:cafe::17]:4711\"", "2001:db8:cafe::17"),
            ("For=\"[2001:db8::1]\"", "2001:db8::1"),
            ("proto=https;for=192.0.2.60", "192.0.2.60"),
        ] {
            let headers = ForwardingHeaders {
                forwarded: &[value],
                ..Default::default()
            };
            assert_eq!(resolve(&headers), Ok(ip(want)), "{value}");
        }
    }

    #[test]
    fn forwarded_takes_the_rightmost_element_that_is_not_a_proxy() {
        let headers = ForwardingHeaders {
            forwarded: &["for=203.0.113.9, for=192.0.2.43", "for=10.0.0.7"],
            ..Default::default()
        };
        assert_eq!(resolve(&headers), Ok(ip("192.0.2.43")));
    }

    #[test]
    fn a_comma_inside_a_quoted_forwarded_value_does_not_split_it() {
        let headers = ForwardingHeaders {
            forwarded: &["by=\"a,b\";for=192.0.2.43"],
            ..Default::default()
        };
        assert_eq!(resolve(&headers), Ok(ip("192.0.2.43")));
    }

    #[test]
    fn an_obfuscated_or_unknown_forwarded_node_is_refused() {
        for value in ["for=_hidden", "for=unknown", "for=\"[::1\"", "for="] {
            let headers = ForwardingHeaders {
                forwarded: &[value],
                ..Default::default()
            };
            assert_eq!(
                resolve(&headers),
                Err(ForwardingError::NotAnAddress("Forwarded")),
                "{value}"
            );
        }
    }

    #[test]
    fn x_real_ip_names_the_client() {
        let headers = ForwardingHeaders {
            x_real_ip: &["192.0.2.43"],
            ..Default::default()
        };
        assert_eq!(resolve(&headers), Ok(ip("192.0.2.43")));
    }

    #[test]
    fn an_x_real_ip_in_a_proxy_network_is_still_the_client() {
        let alone = ForwardingHeaders {
            x_real_ip: &["10.0.0.9"],
            ..Default::default()
        };
        assert_eq!(resolve(&alone), Ok(ip("10.0.0.9")));
        let with_a_chain = ForwardingHeaders {
            x_forwarded_for: &["192.0.2.43, 10.0.0.9"],
            x_real_ip: &["10.0.0.9"],
            ..Default::default()
        };
        assert_eq!(resolve(&with_a_chain), Err(ForwardingError::Conflict));
    }

    #[test]
    fn an_unclosed_quote_cannot_hide_the_address_a_proxy_appended() {
        let headers = ForwardingHeaders {
            forwarded: &["for=192.0.2.43;by=\", for=203.0.113.9"],
            ..Default::default()
        };
        assert_eq!(
            resolve(&headers),
            Err(ForwardingError::NotAnAddress("Forwarded"))
        );
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_quoted_forwarded_value() {
        let headers = ForwardingHeaders {
            forwarded: &["by=\"a\\\"b, c\";for=192.0.2.43"],
            ..Default::default()
        };
        assert_eq!(resolve(&headers), Ok(ip("192.0.2.43")));
    }

    #[test]
    fn x_real_ip_with_a_list_or_a_name_is_refused() {
        for value in ["192.0.2.43, 192.0.2.44", "client.example"] {
            let headers = ForwardingHeaders {
                x_real_ip: &[value],
                ..Default::default()
            };
            assert_eq!(
                resolve(&headers),
                Err(ForwardingError::NotAnAddress("X-Real-IP")),
                "{value}"
            );
        }
    }

    #[test]
    fn headers_that_agree_name_the_client_and_headers_that_differ_are_refused() {
        let agree = ForwardingHeaders {
            forwarded: &["for=192.0.2.43"],
            x_forwarded_for: &["198.51.100.1, 192.0.2.43"],
            x_real_ip: &["192.0.2.43"],
        };
        assert_eq!(resolve(&agree), Ok(ip("192.0.2.43")));
        let differ = ForwardingHeaders {
            x_forwarded_for: &["203.0.113.9"],
            x_real_ip: &["192.0.2.43"],
            ..Default::default()
        };
        assert_eq!(resolve(&differ), Err(ForwardingError::Conflict));
    }

    #[test]
    fn from_a_peer_that_is_not_a_proxy_every_header_is_ignored() {
        let headers = ForwardingHeaders {
            forwarded: &["for=203.0.113.9"],
            x_real_ip: &["203.0.113.9"],
            ..Default::default()
        };
        let client = resolve_client(ip("192.168.1.20"), &headers, &nets(&["10.0.0.0/24"])).unwrap();
        assert_eq!(client.ip, ip("192.168.1.20"));
        assert!(client.ignored_forwarded);
    }

    #[test]
    fn host_names_lose_their_port_brackets_and_trailing_dot() {
        assert_eq!(
            host_name("Buster.local:7126").as_deref(),
            Some("buster.local")
        );
        assert_eq!(host_name("[fe80::1]:7126").as_deref(), Some("fe80::1"));
        assert_eq!(host_name("buster.local.").as_deref(), Some("buster.local"));
        assert_eq!(host_name("").as_deref(), None);
    }
}
