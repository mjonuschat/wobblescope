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
        }
    }

    /// Why the request is refused, or `None` when it may pass.
    fn check(&self, request: &Request) -> Option<ApiError> {
        let Some(peer) = request.extensions().get::<ConnectInfo<SocketAddr>>() else {
            return Some(ApiError::internal(
                "the server did not record who is asking, so the request is refused",
            ));
        };
        let headers = request.headers();
        let client = match self.client(peer.0.ip(), headers) {
            Ok(ip) => ip,
            Err(refusal) => return Some(refusal),
        };
        let authority = authority(request);
        let origin = headers.get(header::ORIGIN);
        match self.moonraker.access() {
            AccessState::Known(policy) => self.check_known(&policy, client, authority, origin),
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
                self.check_host_and_origin(None, authority, origin)
            }
        }
    }

    fn check_known(
        &self,
        policy: &Policy,
        client: IpAddr,
        authority: Option<&str>,
        origin: Option<&header::HeaderValue>,
    ) -> Option<ApiError> {
        if policy.logins_required() {
            return Some(logins_refusal());
        }
        if !policy.is_trusted(client) {
            return Some(ApiError::UntrustedClient {
                message: format!(
                    "{client} is not in Moonraker's trusted_clients; add it there to allow \
                     access"
                ),
            });
        }
        self.check_host_and_origin(Some(policy), authority, origin)
    }

    fn check_host_and_origin(
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
        let allowed = origin.to_str().is_ok_and(|origin| {
            same_origin(origin, authority) || policy.is_some_and(|p| p.origin_allowed(origin))
        });
        (!allowed).then(|| ApiError::CrossOrigin {
            message: "the Origin of this request is not allowed; add it to cors_domains in \
                      moonraker.conf. Behind a reverse proxy, make the proxy forward the \
                      original Host header"
                .to_owned(),
        })
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
    match guard.check(&request) {
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
