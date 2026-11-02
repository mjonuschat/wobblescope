//! Moonraker's access lists, compiled: which clients are trusted, which origins and host names
//! are allowed. The rules copy Moonraker's `authorization` component, so the daemon never
//! admits what Moonraker would refuse. This module does no I/O.

use std::net::IpAddr;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

/// Moonraker's pattern for a host name in `trusted_clients`.
static HOSTNAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[a-z0-9]+(?:-[a-z0-9]+)*\.?)+[a-z]{2,}$").expect("a valid pattern")
});

/// Moonraker's test for a wildcard in the top-level domain of a `cors_domains` entry.
static WILDCARD_TLD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^.+\.[^:]*\*").expect("a valid pattern"));

/// Moonraker's extraction of the host from an origin, for the trusted-IP clause.
static ORIGIN_HOST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https?://([^/:]+)").expect("a valid pattern"));

/// Why the policy could not be built from Moonraker's answer.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct Unreadable(pub String);

/// An IP address or network. A single address is a network with the full prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Net {
    addr: IpAddr,
    prefix: u8,
}

/// Why an entry is not an address or network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// A network whose address has bits set below its prefix.
    HostBits,
    /// Neither an address nor a network.
    Invalid,
}

impl Net {
    /// Parses an address (`192.168.1.5`) or a network (`192.168.0.0/16`), strictly: a network
    /// with host bits set is refused, as Python's `ip_network` does.
    ///
    /// # Errors
    /// [`NetError::HostBits`] or [`NetError::Invalid`].
    pub fn parse(text: &str) -> Result<Self, NetError> {
        if let Ok(addr) = text.parse::<IpAddr>() {
            return Ok(Self {
                addr,
                prefix: max_prefix(addr),
            });
        }
        let (addr, prefix) = text.split_once('/').ok_or(NetError::Invalid)?;
        let addr: IpAddr = addr.parse().map_err(|_| NetError::Invalid)?;
        let prefix = prefix_length(addr, prefix).ok_or(NetError::Invalid)?;
        if prefix > max_prefix(addr) {
            return Err(NetError::Invalid);
        }
        let net = Self { addr, prefix };
        if net.bits() & !net.mask() != 0 {
            return Err(NetError::HostBits);
        }
        Ok(net)
    }

    /// Whether `ip` lies inside. An address of the other family is outside, as in Moonraker.
    #[must_use]
    pub fn contains(&self, ip: IpAddr) -> bool {
        let same_family = matches!(
            (self.addr, ip),
            (IpAddr::V4(_), IpAddr::V4(_)) | (IpAddr::V6(_), IpAddr::V6(_))
        );
        same_family && bits(ip) & self.mask() == self.bits()
    }

    fn bits(&self) -> u128 {
        bits(self.addr)
    }

    fn mask(&self) -> u128 {
        let width = u32::from(max_prefix(self.addr));
        let host = width - u32::from(self.prefix);
        let all = if width == 32 {
            u128::from(u32::MAX)
        } else {
            u128::MAX
        };
        if host >= 128 {
            0
        } else {
            all & !((1u128 << host) - 1)
        }
    }
}

impl std::fmt::Display for Net {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.prefix == max_prefix(self.addr) {
            write!(f, "{}", self.addr)
        } else {
            write!(f, "{}/{}", self.addr, self.prefix)
        }
    }
}

/// The prefix length `text` names: a number, or for IPv4 a dotted netmask (`255.255.255.0`) or
/// hostmask (`0.0.0.255`), as Python's `ip_network` accepts. The netmask reading wins.
fn prefix_length(addr: IpAddr, text: &str) -> Option<u8> {
    if let Ok(prefix) = text.parse::<u8>() {
        return (prefix <= max_prefix(addr)).then_some(prefix);
    }
    let (IpAddr::V4(_), Ok(mask)) = (addr, text.parse::<std::net::Ipv4Addr>()) else {
        return None;
    };
    let mask = u32::from(mask);
    let contiguous = |bits: u32| bits.leading_ones() + bits.trailing_zeros() == 32;
    if contiguous(mask) {
        return u8::try_from(mask.leading_ones()).ok();
    }
    let netmask = !mask;
    contiguous(netmask)
        .then(|| u8::try_from(netmask.leading_ones()).ok())
        .flatten()
}

fn max_prefix(addr: IpAddr) -> u8 {
    match addr {
        IpAddr::V4(_) => 32,
        IpAddr::V6(_) => 128,
    }
}

fn bits(addr: IpAddr) -> u128 {
    match addr {
        IpAddr::V4(a) => u128::from(u32::from(a)),
        IpAddr::V6(a) => u128::from(a),
    }
}

/// An IPv4-mapped IPv6 address as IPv4; any other address unchanged.
#[must_use]
pub fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 @ IpAddr::V4(_) => v4,
    }
}

/// The list an ignored entry came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IgnoredList {
    /// `trusted_clients`.
    TrustedClients,
    /// `cors_domains`, for origins.
    CorsDomains,
    /// `cors_domains`, for the host allowlist only.
    Hosts,
}

/// An entry the policy does not use, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    /// The list it came from.
    pub list: IgnoredList,
    /// The entry's text.
    pub entry: String,
    /// Why it is not used.
    pub reason: String,
}

/// How a host-part pattern matches a host name.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostPattern {
    /// This name.
    Exact(String),
    /// Any name that ends in this suffix, which starts with a dot, and is longer than it.
    Suffix(String),
}

/// The inputs a policy was built from, which decide whether two policies are equal.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    trusted_clients: Vec<String>,
    cors_domains: Vec<String>,
    hostname: Option<String>,
    login_required: bool,
}

/// What Moonraker's lists say about who may use the daemon.
#[derive(Debug, Clone)]
pub struct Policy {
    source: Source,
    trusted: Vec<Net>,
    origins: Vec<Regex>,
    hosts: Vec<HostPattern>,
    ignored: Vec<Ignored>,
}

impl PartialEq for Policy {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

/// The `login_required` value in the `result` of `access.info`, or `None` when it has none.
#[must_use]
pub fn parse_login_required(result: &Value) -> Option<bool> {
    result.get("login_required").and_then(Value::as_bool)
}

/// The host name in the `result` of `printer.info`, when it has one.
#[must_use]
pub fn parse_hostname(result: &Value) -> Option<String> {
    result
        .get("hostname")
        .and_then(Value::as_str)
        .filter(|h| !h.is_empty())
        .map(str::to_owned)
}

impl Policy {
    /// Builds the policy from the `result` of `server.config`, the `login_required` value from
    /// `access.info` and the printer's host name when known.
    ///
    /// # Errors
    /// [`Unreadable`] naming the missing or malformed key.
    pub fn build(
        server_config: &Value,
        login_required: bool,
        hostname: Option<&str>,
    ) -> Result<Self, Unreadable> {
        let auth = server_config
            .get("config")
            .and_then(|c| c.get("authorization"))
            .and_then(Value::as_object)
            .ok_or_else(|| Unreadable("the answer has no config.authorization".to_owned()))?;
        let list = |key: &str| -> Result<Vec<String>, Unreadable> {
            auth.get(key)
                .and_then(Value::as_array)
                .and_then(|items| {
                    items
                        .iter()
                        .map(|v| v.as_str().map(str::to_owned))
                        .collect::<Option<Vec<_>>>()
                })
                .ok_or_else(|| {
                    Unreadable(format!(
                        "the answer has no list of strings at config.authorization.{key}"
                    ))
                })
        };
        let source = Source {
            trusted_clients: list("trusted_clients")?,
            cors_domains: list("cors_domains")?,
            hostname: hostname.map(str::to_lowercase),
            login_required,
        };
        let mut policy = Self {
            source,
            trusted: Vec::new(),
            origins: Vec::new(),
            hosts: Vec::new(),
            ignored: Vec::new(),
        };
        policy.load_trusted();
        policy.load_origins();
        Ok(policy)
    }

    fn ignore(&mut self, list: IgnoredList, entry: &str, reason: &str) {
        self.ignored.push(Ignored {
            list,
            entry: entry.to_owned(),
            reason: reason.to_owned(),
        });
    }

    fn load_trusted(&mut self) {
        for entry in self.source.trusted_clients.clone() {
            match Net::parse(&entry) {
                Ok(net) => self.trusted.push(net),
                Err(NetError::HostBits) => self.ignore(
                    IgnoredList::TrustedClients,
                    &entry,
                    "a network with host bits set, which Moonraker drops",
                ),
                Err(NetError::Invalid) if HOSTNAME.is_match(&entry) => self.ignore(
                    IgnoredList::TrustedClients,
                    &entry,
                    "a domain name, which admits nobody because the daemon does not look up \
                     names",
                ),
                Err(NetError::Invalid) => self.ignore(
                    IgnoredList::TrustedClients,
                    &entry,
                    "neither an address, a network nor a host name",
                ),
            }
        }
    }

    fn load_origins(&mut self) {
        for entry in self.source.cors_domains.clone() {
            if entry.is_empty() || WILDCARD_TLD.is_match(&entry) || entry.ends_with('/') {
                self.ignore(
                    IgnoredList::CorsDomains,
                    &entry,
                    "a wildcard in the top-level domain or a trailing slash, which Moonraker \
                     drops",
                );
                continue;
            }
            let pattern = entry.replace('.', "\\.").replace('*', ".*");
            match Regex::new(&format!(r"\A(?:{pattern})")) {
                Ok(regex) => self.origins.push(regex),
                Err(e) => {
                    let reason = format!("a pattern the daemon cannot compile: {e}");
                    self.ignore(IgnoredList::CorsDomains, &entry, &reason);
                    continue;
                }
            }
            match host_pattern(&entry) {
                Some(pattern) => self.hosts.push(pattern),
                None => self.ignore(
                    IgnoredList::Hosts,
                    &entry,
                    "its host part is not a name or `*.` and a name, so it adds no host to the \
                     allowlist; list the host in allowed_hosts to admit it",
                ),
            }
        }
    }

    /// Whether `ip` is in `trusted_clients`. An IPv4-mapped IPv6 address counts as IPv4.
    #[must_use]
    pub fn is_trusted(&self, ip: IpAddr) -> bool {
        let ip = normalize(ip);
        self.trusted.iter().any(|net| net.contains(ip))
    }

    /// Whether `origin` passes as Moonraker's `check_cors` decides, except for the daemon's own
    /// origin, which the caller compares with `Host`.
    #[must_use]
    pub fn origin_allowed(&self, origin: &str) -> bool {
        if self.origins.is_empty() {
            return false;
        }
        if self
            .origins
            .iter()
            .any(|re| re.find(origin).is_some_and(|m| m.end() == origin.len()))
        {
            return true;
        }
        ORIGIN_HOST
            .captures(origin)
            .and_then(|c| c.get(1))
            .and_then(|m| m.as_str().parse::<IpAddr>().ok())
            .is_some_and(|ip| self.is_trusted(ip))
    }

    /// Whether `host` (lowercase, no port, no trailing dot) matches a host part of
    /// `cors_domains` or is the printer's host name, with or without `.local`.
    #[must_use]
    pub fn host_allowed(&self, host: &str) -> bool {
        if let Some(name) = &self.source.hostname
            && (host == name || host.strip_suffix(".local") == Some(name.as_str()))
        {
            return true;
        }
        self.hosts.iter().any(|pattern| match pattern {
            HostPattern::Exact(name) => host == name,
            HostPattern::Suffix(suffix) => {
                host.len() > suffix.len() && host.ends_with(suffix.as_str())
            }
        })
    }

    /// Whether Moonraker requires logins.
    #[must_use]
    pub fn logins_required(&self) -> bool {
        self.source.login_required
    }

    /// The `trusted_clients` entries as Moonraker listed them.
    #[must_use]
    pub fn trusted_clients(&self) -> &[String] {
        &self.source.trusted_clients
    }

    /// The `cors_domains` entries as Moonraker listed them.
    #[must_use]
    pub fn cors_domains(&self) -> &[String] {
        &self.source.cors_domains
    }

    /// The printer's host name, when Klipper has reported it.
    #[must_use]
    pub fn hostname(&self) -> Option<&str> {
        self.source.hostname.as_deref()
    }

    /// The entries that are not used, each with its reason.
    #[must_use]
    pub fn ignored(&self) -> &[Ignored] {
        &self.ignored
    }

    /// A copy with `login_required` set to `value`, with the same lists.
    #[must_use]
    pub fn with_logins_required(&self, value: bool) -> Self {
        let mut copy = self.clone();
        copy.source.login_required = value;
        copy
    }

    /// A copy with the printer's host name set.
    #[must_use]
    pub fn with_hostname(&self, hostname: &str) -> Self {
        let mut copy = self.clone();
        copy.source.hostname = Some(hostname.to_lowercase());
        copy
    }
}

/// The host-part pattern of a `cors_domains` entry, when it has one of the two shapes the
/// allowlist takes: a literal name, and `*.` followed by a literal name.
fn host_pattern(entry: &str) -> Option<HostPattern> {
    let rest = entry.split_once("://").map_or(entry, |(_, rest)| rest);
    let rest = rest.split('/').next().unwrap_or_default();
    let host = rest.split(':').next().unwrap_or_default().to_lowercase();
    let literal = |name: &str| {
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
    };
    if let Some(suffix) = host.strip_prefix("*.") {
        return literal(suffix).then(|| HostPattern::Suffix(format!(".{suffix}")));
    }
    literal(&host).then_some(HostPattern::Exact(host))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_of_a_v6_network_covers_its_prefix() {
        let net = Net::parse("fe80::/10").unwrap();
        assert!(net.contains("fe80::1".parse().unwrap()));
        assert!(!net.contains("fec0::1".parse().unwrap()));
        let all = Net::parse("::/0").unwrap();
        assert!(all.contains("2001:db8::1".parse().unwrap()));
        let none = Net::parse("0.0.0.0/0").unwrap();
        assert!(none.contains("8.8.8.8".parse().unwrap()));
    }
}
