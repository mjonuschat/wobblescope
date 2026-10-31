//! Refusing writes that a page from another origin sends.
//!
//! The web UI marks every request with `X-Wobblescope-Client`. A page on another origin cannot
//! add a custom header without a CORS preflight, and the daemon answers none, so the browser
//! never sends such a write. A cross-origin form or `text/plain` request needs no preflight but
//! always carries `Origin`, and cannot carry the header.

use axum::extract::Request;
use axum::http::{HeaderMap, Method, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::error::ApiError;

/// The header the web UI sends on every request.
const CLIENT_HEADER: &str = "x-wobblescope-client";

/// The header a browser fills in with the relationship between the page and the target.
const SEC_FETCH_SITE: &str = "sec-fetch-site";

/// Lets reads through. A write is refused when the browser marked it cross-site or same-site,
/// and otherwise passes only with the client header or without an `Origin` (`curl`, scripts).
pub(super) async fn guard(request: Request, next: Next) -> Response {
    if !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    ) && !write_allowed(request.headers())
    {
        return ApiError::CrossOrigin {
            message: "a write from another origin is refused; the web UI sends \
                      X-Wobblescope-Client"
                .to_owned(),
        }
        .into_response();
    }
    next.run(request).await
}

fn write_allowed(headers: &HeaderMap) -> bool {
    let site = headers.get(SEC_FETCH_SITE).and_then(|v| v.to_str().ok());
    if matches!(site, Some("cross-site" | "same-site")) {
        return false;
    }
    headers.contains_key(CLIENT_HEADER) || !headers.contains_key(header::ORIGIN)
}
