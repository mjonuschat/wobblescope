//! Reverse lookup of client addresses, for `trusted_clients` entries that are host names. It
//! follows Moonraker: the name is whatever `getfqdn` finds, compared lowercase and whole, a
//! lookup that takes over five seconds counts for nothing, and a name found is kept for a day.

use std::collections::HashMap;
use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

/// How long Moonraker waits for a lookup.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// How long Moonraker keeps a name it found.
const CACHE_TTL: Duration = Duration::from_secs(84_000);

/// Finds the name an address resolves to.
pub type Lookup = Arc<dyn Fn(IpAddr) -> Pin<Box<dyn Future<Output = String> + Send>> + Send + Sync>;

/// The names addresses resolve to, kept for a while. Clones share the cache.
#[derive(Clone)]
pub struct Names {
    lookup: Lookup,
    timeout: Duration,
    ttl: Duration,
    cache: Arc<Mutex<HashMap<IpAddr, (Instant, String)>>>,
}

impl Names {
    /// Names found by the system resolver.
    #[must_use]
    pub fn system() -> Self {
        Self::with(
            Arc::new(|ip| Box::pin(system_lookup(ip))),
            LOOKUP_TIMEOUT,
            CACHE_TTL,
        )
    }

    /// Names found by `lookup`, with the given timeout and cache lifetime.
    #[must_use]
    pub fn with(lookup: Lookup, timeout: Duration, ttl: Duration) -> Self {
        Self {
            lookup,
            timeout,
            ttl,
            cache: Arc::default(),
        }
    }

    /// The lowercase name `ip` resolves to, or `None` when the lookup took too long. A
    /// timeout is not remembered.
    pub async fn fqdn(&self, ip: IpAddr) -> Option<String> {
        let now = Instant::now();
        if let Some((expires, name)) = self.cache.lock().ok()?.get(&ip)
            && *expires > now
        {
            return Some(name.clone());
        }
        let name = tokio::time::timeout(self.timeout, (self.lookup)(ip))
            .await
            .ok()?
            .to_lowercase();
        if let Ok(mut cache) = self.cache.lock() {
            cache.retain(|_, (expires, _)| *expires > now);
            cache.insert(ip, (now + self.ttl, name.clone()));
        }
        Some(name)
    }
}

/// `getfqdn`: the address's name, or the address itself when it has none.
async fn system_lookup(ip: IpAddr) -> String {
    tokio::task::spawn_blocking(move || getfqdn(ip))
        .await
        .unwrap_or_else(|_| ip.to_string())
}

/// The name Python's `socket.getfqdn` finds: the first of the resolver's host name and its
/// aliases that contains a dot, else the host name.
#[must_use]
pub fn pick_fqdn(hostname: &str, aliases: &[String]) -> String {
    std::iter::once(hostname)
        .chain(aliases.iter().map(String::as_str))
        .find(|name| name.contains('.'))
        .unwrap_or(hostname)
        .to_owned()
}

/// The names the hosts file `text` gives `ip` on its first matching line: the first is the host
/// name and the rest are aliases.
#[must_use]
pub fn hosts_file_names(text: &str, ip: IpAddr) -> Option<(String, Vec<String>)> {
    text.lines().find_map(|line| {
        let line = line.split('#').next().unwrap_or_default();
        let mut fields = line.split_whitespace();
        let listed: IpAddr = fields.next()?.parse().ok()?;
        if listed != ip {
            return None;
        }
        let hostname = fields.next()?.to_owned();
        Some((hostname, fields.map(str::to_owned).collect()))
    })
}

/// The name for `ip`: the hosts file first, as the resolver's `files` source does, and aliases
/// included, because musl's `gethostbyaddr` returns none. Then the resolver.
fn getfqdn(ip: IpAddr) -> String {
    let ip = crate::access::policy::normalize(ip);
    if let Ok(text) = std::fs::read_to_string("/etc/hosts")
        && let Some((hostname, aliases)) = hosts_file_names(&text, ip)
    {
        return pick_fqdn(&hostname, &aliases);
    }
    resolve(ip)
}

/// The resolver's name for `ip` (`getnameinfo`), or the address itself when it has none.
fn resolve(ip: IpAddr) -> String {
    dns_lookup::lookup_addr(&ip).unwrap_or_else(|_| ip.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_name_with_a_dot_wins_as_in_getfqdn() {
        let aliases = vec!["workstation.lan".to_owned(), "ws.example.com".to_owned()];
        assert_eq!(pick_fqdn("workstation", &aliases), "workstation.lan");
        assert_eq!(pick_fqdn("host.lan", &aliases), "host.lan");
        assert_eq!(pick_fqdn("workstation", &["ws".to_owned()]), "workstation");
        assert_eq!(pick_fqdn("workstation", &[]), "workstation");
    }

    #[test]
    fn the_hosts_file_gives_the_first_line_for_an_address_with_its_aliases() {
        let text = "# comment\n127.0.0.1 localhost\n192.168.1.20 workstation workstation.lan # mine\n\
                    192.168.1.20 other\n::1 localhost ip6-localhost\n";
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(
            hosts_file_names(text, ip("192.168.1.20")),
            Some(("workstation".to_owned(), vec!["workstation.lan".to_owned()]))
        );
        assert_eq!(
            hosts_file_names(text, ip("::1")),
            Some(("localhost".to_owned(), vec!["ip6-localhost".to_owned()]))
        );
        assert_eq!(hosts_file_names(text, ip("192.168.1.21")), None);
        assert_eq!(hosts_file_names("192.168.1.9\n", ip("192.168.1.9")), None);
    }

    #[test]
    fn hosts_lines_may_use_tabs_crlf_odd_spacing_and_any_ipv6_spelling() {
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        let text = "10.0.0.5\tbox\tbox.lan\r\n  \r\n\
                    0:0:0:0:0:0:0:1   localhost  \r\n\
                    fe80::1%eth0 zoned\r\n";
        assert_eq!(
            hosts_file_names(text, ip("10.0.0.5")),
            Some(("box".to_owned(), vec!["box.lan".to_owned()]))
        );
        assert_eq!(
            hosts_file_names(text, ip("::1")),
            Some(("localhost".to_owned(), Vec::new()))
        );
        assert_eq!(hosts_file_names(text, ip("fe80::1")), None);
    }

    #[test]
    fn hosts_names_keep_their_case_and_trailing_dot_for_the_caller_to_handle() {
        let ip: IpAddr = "10.0.0.6".parse().unwrap();
        let (host, aliases) = hosts_file_names("10.0.0.6 Box Box.LAN.\n", ip).unwrap();
        assert_eq!(pick_fqdn(&host, &aliases), "Box.LAN.");
    }

    #[tokio::test]
    async fn the_fqdn_is_lowercase_and_a_mapped_client_uses_its_ipv4_name() {
        let names = Names::with(
            Arc::new(|ip| {
                Box::pin(async move {
                    if ip == "10.0.0.7".parse::<IpAddr>().unwrap() {
                        "Box.LAN".to_owned()
                    } else {
                        ip.to_string()
                    }
                })
            }),
            Duration::from_secs(5),
            Duration::from_secs(100),
        );
        let mapped: IpAddr = "10.0.0.7"
            .parse::<std::net::Ipv4Addr>()
            .unwrap()
            .to_ipv6_mapped()
            .into();
        assert_eq!(
            names
                .fqdn(crate::access::policy::normalize(mapped))
                .await
                .as_deref(),
            Some("box.lan")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_name_is_looked_up_again_only_after_its_cache_entry_expires() {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = calls.clone();
        let names = Names::with(
            Arc::new(move |_| {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Box::pin(async { "Host.LAN".to_owned() })
            }),
            Duration::from_secs(5),
            Duration::from_secs(100),
        );
        let ip: IpAddr = "192.168.1.20".parse().unwrap();
        assert_eq!(names.fqdn(ip).await.as_deref(), Some("host.lan"));
        tokio::time::advance(Duration::from_secs(99)).await;
        names.fqdn(ip).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        tokio::time::advance(Duration::from_secs(2)).await;
        names.fqdn(ip).await;
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn the_system_resolver_names_loopback_or_falls_back_to_the_address() {
        let name = Names::system()
            .fqdn("127.0.0.1".parse().unwrap())
            .await
            .unwrap();
        assert!(!name.is_empty());
    }
}
