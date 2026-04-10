use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::Router;
use serde::Deserialize;

use rsdiffy_git::{repo, status};
use rsdiffy_gitlab::{self, GitLabClient, MrComment};

use crate::db;
use crate::server::AppState;
use crate::threads;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/gitlab/details", get(get_details))
        .route("/api/gitlab/push-comments", post(push_comments))
        .route("/api/gitlab/pull-comments", post(pull_comments))
}

async fn get_details(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let remote = match &state.gitlab_remote {
        Some(r) => r,
        None => return Ok(Json(serde_json::Value::Null)),
    };

    let client = GitLabClient::from_env(&remote.base_url).map_err(err500)?;

    // We need the MR IID — parse from the effective_ref if it's an MR URL, or detect from branch
    // For now, try to find the MR for the current source branch
    let branch = repo::get_current_branch();
    let encoded = GitLabClient::encode_project(&remote.project_path);
    let path = format!(
        "/api/v4/projects/{}/merge_requests?source_branch={}&state=opened",
        encoded, branch
    );

    let mrs: Vec<serde_json::Value> = client.get(&path).map_err(err500)?;
    if mrs.is_empty() {
        return Ok(Json(serde_json::Value::Null));
    }

    let mr_iid = mrs[0]["iid"].as_u64().unwrap_or(0);
    let details = rsdiffy_gitlab::fetch_details(&client, &remote.project_path, mr_iid).map_err(err500)?;
    Ok(Json(serde_json::to_value(details).unwrap()))
}

#[derive(Deserialize)]
struct PushCommentsBody {
    comments: Vec<PushCommentItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushCommentItem {
    file_path: String,
    side: String,
    start_line: Option<u32>,
    end_line: u32,
    body: String,
}

async fn push_comments(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PushCommentsBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let remote = state
        .gitlab_remote
        .as_ref()
        .ok_or_else(|| err400("No GitLab remote detected"))?;

    let client = GitLabClient::from_env(&remote.base_url).map_err(err500)?;

    // Find the open MR for current branch
    let branch = repo::get_current_branch();
    let encoded = GitLabClient::encode_project(&remote.project_path);
    let path = format!(
        "/api/v4/projects/{}/merge_requests?source_branch={}&state=opened",
        encoded, branch
    );
    let mrs: Vec<serde_json::Value> = client.get(&path).map_err(err500)?;
    if mrs.is_empty() {
        return Err(err400("No open MR found for current branch"));
    }

    let mr_iid = mrs[0]["iid"].as_u64().unwrap_or(0);
    let details = rsdiffy_gitlab::fetch_details(&client, &remote.project_path, mr_iid).map_err(err500)?;

    let local_head = repo::get_head_hash().map_err(err500)?;
    if local_head != details.head_sha {
        return Err(err409(
            "Local branch is out of sync with the MR. Push or pull your git changes first.",
        ));
    }
    if status::is_dirty().map_err(err500)? {
        return Err(err409(
            "You have uncommitted local changes. Commit or stash them first.",
        ));
    }

    if body.comments.is_empty() {
        return Err(err400("No comments provided"));
    }

    let mr_comments: Vec<MrComment> = body
        .comments
        .into_iter()
        .map(|c| MrComment {
            file_path: c.file_path,
            side: c.side,
            start_line: c.start_line,
            end_line: c.end_line,
            body: c.body,
        })
        .collect();

    let result = rsdiffy_gitlab::push_comments(
        &client,
        &remote.project_path,
        mr_iid,
        &details.head_sha,
        &mr_comments,
    );

    Ok(Json(serde_json::to_value(result).unwrap()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullCommentsBody {
    session_id: Option<String>,
}

async fn pull_comments(
    State(state): State<Arc<AppState>>,
    Json(body): Json<PullCommentsBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let remote = state
        .gitlab_remote
        .as_ref()
        .ok_or_else(|| err400("No GitLab remote detected"))?;

    let sid = body
        .session_id
        .as_deref()
        .ok_or_else(|| err400("Missing sessionId"))?;

    let client = GitLabClient::from_env(&remote.base_url).map_err(err500)?;

    // Find the open MR
    let branch = repo::get_current_branch();
    let encoded = GitLabClient::encode_project(&remote.project_path);
    let path = format!(
        "/api/v4/projects/{}/merge_requests?source_branch={}&state=opened",
        encoded, branch
    );
    let mrs: Vec<serde_json::Value> = client.get(&path).map_err(err500)?;
    if mrs.is_empty() {
        return Err(err400("No open MR found for current branch"));
    }

    let mr_iid = mrs[0]["iid"].as_u64().unwrap_or(0);
    let details = rsdiffy_gitlab::fetch_details(&client, &remote.project_path, mr_iid).map_err(err500)?;

    let local_head = repo::get_head_hash().map_err(err500)?;
    if local_head != details.head_sha {
        return Err(err409(
            "Local branch is out of sync with the MR. Push or pull your git changes first.",
        ));
    }
    if status::is_dirty().map_err(err500)? {
        return Err(err409(
            "You have uncommitted local changes. Commit or stash them first.",
        ));
    }

    let remote_threads =
        rsdiffy_gitlab::pull_comments(&client, &remote.project_path, mr_iid).map_err(err500)?;

    let conn = db::get_db().map_err(err500)?;
    let local_threads = threads::get_threads_for_session(&conn, sid, None).map_err(err500)?;

    let mut pulled: u32 = 0;
    let mut skipped: u32 = 0;

    for rt in &remote_threads {
        let first_comment = match rt.comments.first() {
            Some(c) => c,
            None => continue,
        };

        let already_exists = local_threads.iter().any(|t| {
            t.file_path == rt.file_path
                && t.start_line == rt.start_line.unwrap_or(0) as i64
                && t.end_line == rt.end_line as i64
                && t.comments.iter().any(|c| c.body == first_comment.body)
        });

        if already_exists {
            skipped += 1;
            continue;
        }

        let start_line = rt.start_line.unwrap_or(rt.end_line) as i64;
        let thread = threads::create_thread(
            &conn,
            sid,
            &rt.file_path,
            &rt.side,
            start_line,
            rt.end_line as i64,
            &first_comment.body,
            &first_comment.author,
            "user",
            None,
        )
        .map_err(err500)?;

        for comment in rt.comments.iter().skip(1) {
            threads::add_reply(&conn, &thread.id, &comment.body, &comment.author, "user")
                .map_err(err500)?;
        }

        pulled += 1;
    }

    Ok(Json(serde_json::json!({ "pulled": pulled, "skipped": skipped })))
}

fn err400(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn err409(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::CONFLICT,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn err500(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}
