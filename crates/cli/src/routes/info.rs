use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde::Serialize;

use rsdiffy_git::{diff, repo, status, commits, CommitQuery};

use crate::server::AppState;
use crate::session;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/info", get(get_info))
        .route("/api/overview", get(get_overview))
        .route("/api/commits", get(get_commits))
}

#[derive(serde::Deserialize)]
struct InfoQuery {
    #[serde(rename = "ref")]
    git_ref: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InfoResponse {
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

async fn get_info(
    State(state): State<Arc<AppState>>,
    Query(q): Query<InfoQuery>,
) -> Result<Json<InfoResponse>, (StatusCode, Json<serde_json::Value>)> {
    let info = repo::get_repo_info().map_err(err500)?;

    let effective = q.git_ref.as_deref().unwrap_or(&state.effective_ref);

    let description = if q.git_ref.is_some() {
        description_for_ref(effective)
    } else {
        state.description.clone()
    };

    let capabilities = repo::get_ref_capabilities(Some(effective));

    let session_id = if !effective.is_empty() {
        let head_hash = repo::get_head_hash().unwrap_or_default();
        session::find_or_create_session(&state.rsdiffy_dir, effective, &head_hash)
            .ok()
            .map(|s| s.id)
    } else {
        None
    };

    let gitlab = state.gitlab_remote.as_ref().map(|r| GitLabInfo {
        base_url: r.base_url.clone(),
        project_path: r.project_path.clone(),
    });

    Ok(Json(InfoResponse {
        name: info.name,
        branch: info.branch,
        root: info.root,
        description,
        capabilities,
        session_id,
        gitlab,
        editor: state.editor_available.clone(),
    }))
}

#[derive(Serialize)]
struct OverviewFile {
    path: String,
    status: String,
}

#[derive(Serialize)]
struct OverviewResponse {
    files: Vec<OverviewFile>,
}

async fn get_overview() -> Result<Json<OverviewResponse>, (StatusCode, Json<serde_json::Value>)> {
    let staged = status::get_staged_files().map_err(err500)?;
    let unstaged = status::get_unstaged_files().map_err(err500)?;
    let untracked = diff::get_untracked_files().map_err(err500)?;

    let mut file_map: HashMap<String, String> = HashMap::new();
    for f in staged {
        file_map.insert(f, "staged".to_string());
    }
    for f in unstaged {
        file_map.insert(f, "modified".to_string());
    }
    for f in untracked {
        file_map.insert(f, "added".to_string());
    }

    let files: Vec<OverviewFile> = file_map
        .into_iter()
        .map(|(path, status)| OverviewFile { path, status })
        .collect();

    Ok(Json(OverviewResponse { files }))
}

#[derive(serde::Deserialize)]
struct CommitsQuery {
    count: Option<u32>,
    skip: Option<u32>,
    search: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CommitsResponse {
    commits: Vec<rsdiffy_git::Commit>,
    has_more: bool,
}

async fn get_commits(
    Query(q): Query<CommitsQuery>,
) -> Result<Json<CommitsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let count = q.count.unwrap_or(10);
    let query = CommitQuery {
        count,
        skip: q.skip.unwrap_or(0),
        search: q.search,
    };
    let result = commits::get_recent_commits(&query).map_err(err500)?;
    let has_more = result.len() as u32 == count;
    Ok(Json(CommitsResponse {
        commits: result,
        has_more,
    }))
}

fn description_for_ref(git_ref: &str) -> String {
    match git_ref {
        "staged" => "Staged changes".to_string(),
        "unstaged" => "Unstaged changes".to_string(),
        "work" | "." => "All changes".to_string(),
        r if r.contains("..") => r.to_string(),
        r => format!("Changes from {}", r),
    }
}

fn err500(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}
