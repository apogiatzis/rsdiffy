use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::post;
use axum::Router;
use serde::Deserialize;

use rsdiffy_git::{diff, repo};

use crate::server::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/revert-file", post(revert_file))
        .route("/api/revert-hunk", post(revert_hunk))
        .route("/api/open-in-editor", post(open_in_editor))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RevertFileBody {
    file_path: Option<String>,
    is_untracked: Option<bool>,
}

async fn revert_file(
    Json(body): Json<RevertFileBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let path = body
        .file_path
        .as_deref()
        .ok_or_else(|| err400("Missing filePath"))?;
    // Validate path is within repo to prevent traversal attacks
    repo::validate_repo_path(path).map_err(err400_git)?;
    diff::revert_file(path, body.is_untracked.unwrap_or(false)).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct RevertHunkBody {
    patch: Option<String>,
}

async fn revert_hunk(
    Json(body): Json<RevertHunkBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let patch = body.patch.as_deref().ok_or_else(|| err400("Missing patch"))?;
    diff::revert_hunk(patch).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenInEditorBody {
    file_path: Option<String>,
    line: Option<u32>,
}

async fn open_in_editor(
    State(state): State<Arc<AppState>>,
    Json(body): Json<OpenInEditorBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let editor = state
        .editor_available
        .as_deref()
        .ok_or_else(|| err404("No editor available"))?;

    let file_path = body.file_path.as_deref().unwrap_or("");
    let repo_root = repo::get_repo_info().map_err(err500)?.root;

    let full_path = if file_path.is_empty() {
        repo_root.clone()
    } else {
        let canonical = repo::validate_repo_path(file_path).map_err(err400_git)?;
        canonical.to_string_lossy().to_string()
    };

    let goto_arg = if let Some(line) = body.line {
        format!("{}:{}", full_path, line)
    } else {
        full_path
    };

    if editor == "vscode" {
        std::process::Command::new("code")
            .args([&repo_root, "--goto", &goto_arg])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(err500)?;
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

fn err400(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn err400_git(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
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
