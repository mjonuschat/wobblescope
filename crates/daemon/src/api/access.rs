//! The outermost guard: every request, whatever it asks for, is checked against what Moonraker
//! trusts before any route sees it.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ApiError;
use crate::access::client::{host_allowed, resolve_client, same_origin};
use crate::access::names::Names;
use crate::access::policy::Net;
use crate::access::{AccessState, Policy};
use crate::moonraker::Moonraker;

const FORWARDED_FOR: &str = "x-forwarded-for";

/// What the guard needs: Moonraker's published lists and the daemon's own settings. Clones
/// share the once-only warning.
#[derive(Clone)]
pub struct AccessGuard {
    moonraker: Moonraker,
    proxies: Arc<[Net]>,
    allowed_hosts: Arc<[String]>,
    warned_forwarded: Arc<AtomicBool>,
    names: Names,
}

impl AccessGuard {
    /// A guard that reads the access state from `moonraker`, trusts `X-Forwarded-For` only from
    /// `proxies`, and admits the host names `allowed_hosts` (lowercase).
    #[must_use]
    pub fn new(moonraker: Moonraker, proxies: Vec<Net>, allowed_hosts: Vec<String>) -> Self {
        Self {
            moonraker,
            proxies: proxies.into(),
            allowed_hosts: allowed_hosts.into(),
            warned_forwarded: Arc::new(AtomicBool::new(false)),
            names: Names::system(),
        }
    }

    /// The same guard with another source of names for reverse lookups.
    #[must_use]
    pub fn with_names(mut self, names: Names) -> Self {
        self.names = names;
        self
    }

    /// Whether `trusted_clients` admits `ip`: by address or network, or else by the name the
    /// address resolves to when the list has host names.
    async fn trusted(&self, policy: &Policy, ip: IpAddr) -> bool {
        if policy.is_trusted(ip) {
            return true;
        }
        if !policy.has_domains() {
            return false;
        }
        match self.names.fqdn(crate::access::policy::normalize(ip)).await {
            Some(name) => policy.is_trusted_name(&name),
            None => false,
        }
    }

    /// Why the request is refused, or `None` when it may pass.
    /// What the checks read from `request`, copied so that the request, whose body is not
    /// `Sync`, is not held across a lookup.
    fn parts(request: &Request) -> Result<(IpAddr, HeaderMap, Option<String>), ApiError> {
        let peer = request
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip())
            .ok_or_else(|| {
                ApiError::internal(
                    "the server did not record who is asking, so the request is refused",
                )
            })?;
        Ok((
            peer,
            request.headers().clone(),
            authority(request).map(str::to_owned),
        ))
    }

    async fn check_parts(
        &self,
        peer: IpAddr,
        headers: &HeaderMap,
        authority: Option<&str>,
    ) -> Option<ApiError> {
        let client = match self.client(peer, headers) {
            Ok(ip) => ip,
            Err(refusal) => return Some(refusal),
        };
        let origin = headers.get(header::ORIGIN);
        match self.moonraker.access() {
            AccessState::Known(policy) => {
                self.check_known(&policy, client, authority, origin).await
            }
            AccessState::Unknown { logins_required } => {
                if logins_required {
                    return Some(logins_refusal());
                }
                if !client.is_loopback() {
                    return Some(ApiError::AccessUnavailable {
                        message: "the daemon has not read Moonraker's trusted_clients yet, so it \
                                  admits only loopback clients; see the daemon's log"
                            .to_owned(),
                    });
                }
                self.check_host_and_origin(None, authority, origin).await
            }
        }
    }

    async fn check_known(
        &self,
        policy: &Policy,
        client: IpAddr,
        authority: Option<&str>,
        origin: Option<&header::HeaderValue>,
    ) -> Option<ApiError> {
        if policy.logins_required() {
            return Some(logins_refusal());
        }
        if !self.trusted(policy, client).await {
            return Some(ApiError::UntrustedClient {
                message: format!(
                    "{client} is not in Moonraker's trusted_clients; add it there to allow \
                     access"
                ),
            });
        }
        self.check_host_and_origin(Some(policy), authority, origin)
            .await
    }

    async fn check_host_and_origin(
        &self,
        policy: Option<&Policy>,
        authority: Option<&str>,
        origin: Option<&header::HeaderValue>,
    ) -> Option<ApiError> {
        let Some(authority) = authority.filter(|a| host_allowed(a, policy, &self.allowed_hosts))
        else {
            return Some(ApiError::HostNotAllowed {
                message: "the Host of this request is not allowed, which guards against DNS \
                          rebinding; open the UI by a name Moonraker's cors_domains lists, or \
                          add the name to allowed_hosts in the daemon's config"
                    .to_owned(),
            });
        };
        let origin = origin?;
        let allowed = match origin.to_str() {
            Ok(origin) => self.origin_allowed(policy, origin, authority).await,
            Err(_) => false,
        };
        (!allowed).then(|| ApiError::CrossOrigin {
            message: "the Origin of this request is not allowed; add it to cors_domains in \
                      moonraker.conf. Behind a reverse proxy, make the proxy forward the \
                      original Host header"
                .to_owned(),
        })
    }

    async fn origin_allowed(&self, policy: Option<&Policy>, origin: &str, authority: &str) -> bool {
        if same_origin(origin, authority) {
            return true;
        }
        let Some(policy) = policy else {
            return false;
        };
        if policy.origin_matches_pattern(origin) {
            return true;
        }
        match policy.origin_host_ip(origin) {
            Some(ip) => self.trusted(policy, ip).await,
            None => false,
        }
    }

    /// The client's address: the peer, or the forwarded address when the peer is a proxy.
    fn client(&self, peer: IpAddr, headers: &HeaderMap) -> Result<IpAddr, ApiError> {
        let forwarded: Vec<&str> = headers
            .get_all(FORWARDED_FOR)
            .iter()
            .map(|v| v.to_str().unwrap_or_default())
            .collect();
        let refusal = || ApiError::UntrustedClient {
            message: "X-Forwarded-For holds something that is not an IP address".to_owned(),
        };
        let client = resolve_client(peer, &forwarded, &self.proxies).map_err(|_| refusal())?;
        if client.ignored_forwarded && !self.warned_forwarded.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "a request from {peer} carries X-Forwarded-For, which is ignored because {peer} \
                 is not in trusted_proxies; if a reverse proxy runs there, list it in \
                 trusted_proxies"
            );
        }
        Ok(client.ip)
    }
}

fn logins_refusal() -> ApiError {
    ApiError::LoginsRequired {
        message: "Moonraker requires logins (force_logins) and wobblescope cannot check logins \
                  yet; turn force_logins off in moonraker.conf to use it"
            .to_owned(),
    }
}

/// The `Host` header, or the authority of the request URI (HTTP/2).
fn authority(request: &Request) -> Option<&str> {
    request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            request
                .uri()
                .authority()
                .map(axum::http::uri::Authority::as_str)
        })
}

pub(super) async fn guard(
    State(guard): State<AccessGuard>,
    request: Request,
    next: Next,
) -> Response {
    let refusal = match AccessGuard::parts(&request) {
        Ok((peer, headers, authority)) => {
            guard
                .check_parts(peer, &headers, authority.as_deref())
                .await
        }
        Err(refusal) => Some(refusal),
    };
    match refusal {
        None => next.run(request).await,
        Some(refusal) => {
            tracing::debug!(
                "refused {} {}: {}",
                request.method(),
                request.uri().path(),
                refusal_code(&refusal)
            );
            refusal.into_response()
        }
    }
}

fn refusal_code(error: &ApiError) -> String {
    serde_json::to_value(error)
        .ok()
        .and_then(|v| v.get("error").and_then(|c| c.as_str().map(str::to_owned)))
        .unwrap_or_default()
}
