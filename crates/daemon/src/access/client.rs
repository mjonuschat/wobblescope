//! The pure parts of the guard: who the client is, and whether the `Host` and `Origin` of a
//! request are acceptable. Nothing here knows about HTTP types.

use std::net::IpAddr;

use super::policy::{Net, Policy, normalize};

/// Who a request comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Client {
    /// The client's address, with an IPv4-mapped address turned into IPv4.
    pub ip: IpAddr,
    /// Whether the peer sent `X-Forwarded-For` although it is not a configured proxy, so the
    /// header was ignored.
    pub ignored_forwarded: bool,
}

/// An `X-Forwarded-For` entry that is not an IP address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadForwarded;

/// Resolves the client from the TCP `peer` and the values of every `X-Forwarded-For` header.
///
/// The peer is the client unless it is in `proxies`. Then the client is the rightmost entry
/// that is not a proxy, or the leftmost entry when all of them are.
///
/// # Errors
/// [`BadForwarded`] when the peer is a proxy and any entry is not an IP address.
pub fn resolve_client(
    peer: IpAddr,
    forwarded: &[&str],
    proxies: &[Net],
) -> Result<Client, BadForwarded> {
    let peer = normalize(peer);
    let is_proxy = |ip: IpAddr| proxies.iter().any(|net| net.contains(ip));
    if !is_proxy(peer) {
        return Ok(Client {
            ip: peer,
            ignored_forwarded: !forwarded.is_empty(),
        });
    }
    let mut entries = Vec::new();
    for value in forwarded {
        for entry in value.split(',') {
            let ip: IpAddr = entry.trim().parse().map_err(|_| BadForwarded)?;
            entries.push(normalize(ip));
        }
    }
    let ip = entries
        .iter()
        .rev()
        .find(|ip| !is_proxy(**ip))
        .or(entries.first())
        .copied()
        .unwrap_or(peer);
    Ok(Client {
        ip,
        ignored_forwarded: false,
    })
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
