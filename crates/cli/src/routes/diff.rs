use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};

use rsdiffy_git::diff;
use rsdiffy_parser::parse_diff;

use crate::server::AppState;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/diff", get(get_diff))
        .route("/api/diff/ref", get(get_diff_ref))
        .route("/api/diff-fingerprint", get(get_diff_fingerprint))
        .route("/api/file/{*path}", get(get_file))
}

#[derive(Deserialize)]
struct DiffQuery {
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    whitespace: Option<String>,
}

async fn get_diff(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DiffQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let extra_args: Vec<&str> = if q.whitespace.as_deref() == Some("hide") {
        vec!["-w"]
    } else {
        vec![]
    };

    let base_ref = q
        .git_ref
        .as_deref()
        .map(|r| diff::resolve_base_ref(r).unwrap_or_else(|_| "HEAD".to_string()))
        .unwrap_or_else(|| "HEAD".to_string());

    let raw = if let Some(ref git_ref) = q.git_ref {
        diff::resolve_ref(git_ref, &extra_args).map_err(err500)?
    } else {
        let args: Vec<&str> = state.diff_args.iter().map(|s| s.as_str()).collect();
        let mut all_args = args;
        all_args.extend_from_slice(&extra_args);
        get_full_diff(&state, &all_args).map_err(err500)?
    };

    let mut parsed = parse_diff(&raw);
    enrich_with_line_counts(&mut parsed, &base_ref);
    Ok(Json(serde_json::to_value(parsed).unwrap()))
}

#[derive(Deserialize)]
struct RefQuery {
    #[serde(rename = "ref")]
    git_ref: Option<String>,
}

#[derive(Serialize)]
struct DiffRefResponse {
    args: String,
}

async fn get_diff_ref(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RefQuery>,
) -> Json<DiffRefResponse> {
    if let Some(ref git_ref) = q.git_ref {
        let resolved = diff::resolve_diff_args(git_ref);
        Json(DiffRefResponse {
            args: resolved.args.join(" "),
        })
    } else {
        let args = if state.diff_args.is_empty() {
            vec!["HEAD".to_string()]
        } else {
            state.diff_args.clone()
        };
        Json(DiffRefResponse {
            args: args.join(" "),
        })
    }
}

#[derive(Serialize)]
struct FingerprintResponse {
    fingerprint: String,
}

async fn get_diff_fingerprint(
    State(state): State<Arc<AppState>>,
    Query(q): Query<RefQuery>,
) -> Json<FingerprintResponse> {
    let stat = if let Some(ref git_ref) = q.git_ref {
        diff::get_diff_stat_for_ref(git_ref)
    } else {
        let args: Vec<&str> = state.diff_args.iter().map(|s| s.as_str()).collect();
        let mut stat = diff::get_diff_stat(&args);
        if state.include_untracked {
            if let Ok(untracked) = diff::get_untracked_files() {
                if !untracked.is_empty() {
                    stat.push('\n');
                    stat.push_str(&untracked.join("\n"));
                }
            }
        }
        stat
    };

    Json(FingerprintResponse {
        fingerprint: sha1_short(&stat),
    })
}

#[derive(Serialize)]
struct FileResponse {
    path: String,
    content: Vec<String>,
}

async fn get_file(
    Path(file_path): Path<String>,
    Query(q): Query<RefQuery>,
) -> Result<Json<FileResponse>, (StatusCode, Json<serde_json::Value>)> {
    let base_ref = q
        .git_ref
        .as_deref()
        .map(|r| diff::resolve_base_ref(r).unwrap_or_else(|_| "HEAD".to_string()))
        .unwrap_or_else(|| "HEAD".to_string());

    let content = diff::get_file_content(&file_path, &base_ref)
        .map_err(|_| err404(&format!("File not found: {}", file_path)))?;

    Ok(Json(FileResponse {
        path: file_path,
        content: content.split('\n').map(|s| s.to_string()).collect(),
    }))
}

fn get_full_diff(state: &AppState, args: &[&str]) -> rsdiffy_git::Result<String> {
    let mut raw = diff::get_diff(args)?;
    if state.include_untracked {
        let untracked_files = diff::get_untracked_files()?;
        if !untracked_files.is_empty() {
            let untracked_diff = diff::get_untracked_diff(&untracked_files);
            if !untracked_diff.is_empty() {
                raw.push('\n');
                raw.push_str(&untracked_diff);
            }
        }
    }
    Ok(raw)
}

fn enrich_with_line_counts(parsed: &mut rsdiffy_parser::ParsedDiff, base_ref: &str) {
    for file in &mut parsed.files {
        if file.status == rsdiffy_parser::FileStatus::Added || file.is_binary {
            continue;
        }
        let path = if file.old_path.is_empty() {
            &file.new_path
        } else {
            &file.old_path
        };
        if let Some(count) = diff::get_file_line_count(path, base_ref) {
            file.old_file_line_count = Some(count);
        }
    }
}

fn sha1_short(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:012x}", hasher.finish())
}

fn err500(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}

fn err404(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({ "error": msg })),
    )
}
