use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use axum::Router;
use serde::Serialize;

use rsdiffy_git::{repo, tree};

use crate::server::AppState;
use crate::session;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tree", get(get_tree))
        .route("/api/tree/fingerprint", get(get_tree_fingerprint))
        .route("/api/tree/entries", get(get_tree_entries))
        .route("/api/tree/info", get(get_tree_info))
        .route("/api/tree/file/{*path}", get(get_tree_file))
        .route("/api/tree/raw/{*path}", get(get_tree_raw))
}

#[derive(Serialize)]
struct TreeResponse {
    paths: Vec<String>,
}

async fn get_tree() -> Result<Json<TreeResponse>, (StatusCode, Json<serde_json::Value>)> {
    let paths = tree::get_tree().map_err(err500)?;
    Ok(Json(TreeResponse { paths }))
}

#[derive(Serialize)]
struct FingerprintResponse {
    fingerprint: String,
}

async fn get_tree_fingerprint() -> Result<Json<FingerprintResponse>, (StatusCode, Json<serde_json::Value>)> {
    let raw = tree::get_tree_fingerprint().map_err(err500)?;
    Ok(Json(FingerprintResponse {
        fingerprint: sha1_short(&raw),
    }))
}

#[derive(serde::Deserialize)]
struct EntriesQuery {
    path: Option<String>,
}

#[derive(Serialize)]
struct EntriesResponse {
    entries: Vec<rsdiffy_git::TreeEntry>,
}

async fn get_tree_entries(
    Query(q): Query<EntriesQuery>,
) -> Result<Json<EntriesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let entries = tree::get_tree_entries("HEAD", q.path.as_deref()).map_err(err500)?;
    Ok(Json(EntriesResponse { entries }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeInfoResponse {
    name: String,
    branch: String,
    root: String,
    description: String,
    capabilities: rsdiffy_git::RefCapabilities,
    session_id: Option<String>,
    gitlab: Option<GitLabInfo>,
    editor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GitLabInfo {
    base_url: String,
    project_path: String,
}

async fn get_tree_info(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TreeInfoResponse>, (StatusCode, Json<serde_json::Value>)> {
    let info = repo::get_repo_info().map_err(err500)?;
    let head_hash = repo::get_head_hash().unwrap_or_default();
    let session_id = session::find_or_create_session(&state.rsdiffy_dir, "__tree__", &head_hash)
        .ok()
        .map(|s| s.id);

    let gitlab = state.gitlab_remote.as_ref().map(|r| GitLabInfo {
        base_url: r.base_url.clone(),
        project_path: r.project_path.clone(),
    });

    Ok(Json(TreeInfoResponse {
        name: info.name,
        branch: info.branch,
        root: info.root,
        description: "Repository file browser".to_string(),
        capabilities: rsdiffy_git::RefCapabilities {
            reviews: true,
            revert: false,
            staleness: false,
        },
        session_id,
        gitlab,
        editor: state.editor_available.clone(),
    }))
}

#[derive(Serialize)]
struct FileResponse {
    path: String,
    content: Vec<String>,
}

async fn get_tree_file(
    Path(file_path): Path<String>,
) -> Result<Json<FileResponse>, (StatusCode, Json<serde_json::Value>)> {
    let content = tree::get_working_tree_file_content(&file_path)
        .map_err(|_| err404(&format!("File not found: {}", file_path)))?;

    Ok(Json(FileResponse {
        path: file_path,
        content: content.split('\n').map(|s| s.to_string()).collect(),
    }))
}

const MIME_TYPES: &[(&str, &str)] = &[
    (".html", "text/html"),
    (".js", "application/javascript"),
    (".css", "text/css"),
    (".json", "application/json"),
    (".svg", "image/svg+xml"),
    (".png", "image/png"),
    (".jpg", "image/jpeg"),
    (".jpeg", "image/jpeg"),
    (".gif", "image/gif"),
    (".webp", "image/webp"),
    (".avif", "image/avif"),
    (".ico", "image/x-icon"),
    (".pdf", "application/pdf"),
];

fn guess_mime(path: &str) -> &'static str {
    for (ext, mime) in MIME_TYPES {
        if path.ends_with(ext) {
            return mime;
        }
    }
    "application/octet-stream"
}

async fn get_tree_raw(
    Path(file_path): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let (data, _) = tree::get_working_tree_raw_file(&file_path)
        .map_err(|_| err404(&format!("File not found: {}", file_path)))?;

    let mime = guess_mime(&file_path);
    Ok((
        StatusCode::OK,
        [(axum::http::header::CONTENT_TYPE, mime)],
        data,
    )
        .into_response())
}

fn sha1_short(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:012x}", hasher.finish())
}

fn err404(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn err500(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}
