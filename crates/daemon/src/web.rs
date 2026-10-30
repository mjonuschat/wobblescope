use axum::{
    http::{StatusCode, Uri, header},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
struct Assets;

/// Serves embedded UI files, falling back to `index.html` for unknown paths.
pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    let (path, file) = match Assets::get(path) {
        Some(file) => (path, file),
        None => match Assets::get("index.html") {
            Some(file) => ("index.html", file),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    (
        [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
        file.data,
    )
        .into_response()
}
