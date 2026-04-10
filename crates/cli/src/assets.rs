use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "../../ui/"]
struct UiAssets;

/// Serve embedded static UI assets.
/// Falls back to index.html for SPA routing (any path that doesn't match an asset).
pub async fn static_handler(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');

    if let Some(content) = UiAssets::get(path) {
        let mime = mime_guess::from_path(path)
            .first_or_octet_stream()
            .to_string();
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, mime)],
            content.data.to_vec(),
        )
            .into_response();
    }

    match UiAssets::get("index.html") {
        Some(content) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html".to_string())],
            content.data.to_vec(),
        )
            .into_response(),
        None => (StatusCode::NOT_FOUND, "UI assets not found").into_response(),
    }
}
