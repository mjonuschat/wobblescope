mod common;

use axum::http::{StatusCode, header};
use common::{cell, idle_moonraker, request, send};
use wobblescope::api::mount;
use wobblescope::web::static_handler;

// nginx serves the daemon under /wobblescope/ and strips the prefix, so the daemon sees `/`.
// The page must then name its files relative to itself, or the browser asks the host root.
#[tokio::test]
async fn the_index_names_its_files_relative_and_each_one_is_served() {
    let app = mount(cell(None), idle_moonraker()).fallback(static_handler);
    let (status, headers, body) = send(&app, request("GET", "/")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "text/html");
    let html = String::from_utf8(body).unwrap();

    let urls: Vec<&str> = html
        .split(['"', '\''])
        .filter(|part| part.starts_with("./") || part.starts_with('/'))
        .filter(|part| part.contains('.') && !part.contains(' '))
        .collect();
    assert!(
        urls.iter()
            .any(|u| u.starts_with("./assets/") && u.ends_with(".js")),
        "{urls:?}"
    );
    assert!(urls.iter().any(|u| u.ends_with(".css")), "{urls:?}");
    for url in &urls {
        assert!(url.starts_with("./"), "{url} is root-relative");
        let path = format!("/{}", url.trim_start_matches("./"));
        let (status, headers, body) = send(&app, request("GET", &path)).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        assert!(!body.is_empty(), "{path}");
        assert_ne!(
            headers[header::CONTENT_TYPE],
            "text/html",
            "{path} fell back to the index"
        );
    }
}
