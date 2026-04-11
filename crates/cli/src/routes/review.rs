use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{delete, get, patch, post};
use axum::Router;
use serde::Deserialize;

use crate::agent;
use crate::db;
use crate::server::AppState;
use crate::session;
use crate::threads;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/sessions/current", get(get_current_session))
        .route(
            "/api/threads",
            get(get_threads).post(create_thread).delete(delete_all_threads),
        )
        .route("/api/threads/{id}/reply", post(add_reply))
        .route("/api/threads/{id}/status", patch(update_thread_status))
        .route("/api/threads/{id}", delete(delete_thread))
        .route(
            "/api/comments/{id}",
            patch(edit_comment).delete(delete_comment),
        )
}

async fn get_current_session(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    match session::get_current_session(&state.rsdiffy_dir) {
        Ok(Some(s)) => Json(serde_json::to_value(s).unwrap()),
        _ => Json(serde_json::Value::Null),
    }
}

#[derive(Deserialize)]
struct ThreadsQuery {
    session: Option<String>,
    status: Option<String>,
}

async fn get_threads(
    Query(q): Query<ThreadsQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sid = q.session.as_deref().ok_or_else(|| err400("Missing session parameter"))?;
    let conn = db::get_db().map_err(err500)?;
    let result = threads::get_threads_for_session(&conn, sid, q.status.as_deref()).map_err(err500)?;
    Ok(Json(serde_json::to_value(result).unwrap()))
}

#[derive(Deserialize)]
struct CreateThreadBody {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "filePath")]
    file_path: Option<String>,
    side: Option<String>,
    #[serde(rename = "startLine")]
    start_line: Option<i64>,
    #[serde(rename = "endLine")]
    end_line: Option<i64>,
    body: Option<String>,
    author: Option<AuthorBody>,
    #[serde(rename = "anchorContent")]
    anchor_content: Option<String>,
}

#[derive(Deserialize)]
struct AuthorBody {
    name: String,
    #[serde(rename = "type")]
    author_type: String,
}

async fn create_thread(
    State(_state): State<Arc<AppState>>,
    Json(body): Json<CreateThreadBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sid = body.session_id.as_deref().ok_or_else(|| err400("Missing sessionId"))?;
    let file_path = body.file_path.as_deref().ok_or_else(|| err400("Missing filePath"))?;
    let side = body.side.as_deref().ok_or_else(|| err400("Missing side"))?;
    let start_line = body.start_line.ok_or_else(|| err400("Missing startLine"))?;
    let end_line = body.end_line.ok_or_else(|| err400("Missing endLine"))?;
    let comment_body = body.body.as_deref().ok_or_else(|| err400("Missing body"))?;
    let author = body.author.as_ref().ok_or_else(|| err400("Missing author"))?;

    let conn = db::get_db().map_err(err500)?;
    let thread = threads::create_thread(
        &conn,
        sid,
        file_path,
        side,
        start_line,
        end_line,
        comment_body,
        &author.name,
        &author.author_type,
        body.anchor_content.as_deref(),
    )
    .map_err(err500)?;

    // Detect @agent mention in the initial comment
    if let Some((agent_name, instruction)) = agent::detect_agent_mention(comment_body) {
        let prompt = build_prompt_from_thread(&thread, &instruction);
        let thread_id = thread.id.clone();
        tokio::spawn(agent::spawn_agent_reply(thread_id, agent_name, prompt));
    }

    Ok(Json(serde_json::to_value(thread).unwrap()))
}

#[derive(Deserialize)]
struct DeleteAllBody {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

async fn delete_all_threads(
    Json(body): Json<DeleteAllBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sid = body.session_id.as_deref().ok_or_else(|| err400("Missing sessionId"))?;
    let conn = db::get_db().map_err(err500)?;
    threads::delete_all_threads_for_session(&conn, sid).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct ReplyBody {
    body: Option<String>,
    author: Option<AuthorBody>,
}

async fn add_reply(
    State(_state): State<Arc<AppState>>,
    Path(thread_id): Path<String>,
    Json(body): Json<ReplyBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let comment_body = body.body.as_deref().ok_or_else(|| err400("Missing body"))?;
    let author = body.author.as_ref().ok_or_else(|| err400("Missing author"))?;

    let conn = db::get_db().map_err(err500)?;
    let comment = threads::add_reply(&conn, &thread_id, comment_body, &author.name, &author.author_type)
        .map_err(err500)?;

    // Detect @agent mention in the reply
    if let Some((agent_name, instruction)) = agent::detect_agent_mention(comment_body) {
        if let Ok(thread) = threads::get_thread(&conn, &thread_id) {
            let prompt = build_prompt_from_thread(&thread, &instruction);
            let tid = thread_id.clone();
            tokio::spawn(agent::spawn_agent_reply(tid, agent_name, prompt));
        }
    }

    Ok(Json(serde_json::to_value(comment).unwrap()))
}

#[derive(Deserialize)]
struct StatusBody {
    status: Option<String>,
    summary: Option<String>,
}

async fn update_thread_status(
    Path(thread_id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let status = body.status.as_deref().ok_or_else(|| err400("Missing status"))?;
    let conn = db::get_db().map_err(err500)?;

    let (summary_body, summary_name, summary_type) = if let Some(ref s) = body.summary {
        (Some(s.as_str()), Some("System"), Some("user"))
    } else {
        (None, None, None)
    };

    threads::update_thread_status(&conn, &thread_id, status, summary_body, summary_name, summary_type)
        .map_err(err500)?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn delete_thread(
    Path(thread_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let conn = db::get_db().map_err(err500)?;
    threads::delete_thread(&conn, &thread_id).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct EditCommentBody {
    body: Option<String>,
}

async fn edit_comment(
    Path(comment_id): Path<String>,
    Json(body): Json<EditCommentBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let comment_body = body.body.as_deref().ok_or_else(|| err400("Missing body"))?;
    let conn = db::get_db().map_err(err500)?;
    threads::edit_comment(&conn, &comment_id, comment_body).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn delete_comment(
    Path(comment_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let conn = db::get_db().map_err(err500)?;
    threads::delete_comment(&conn, &comment_id).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

fn build_prompt_from_thread(thread: &threads::Thread, instruction: &str) -> String {
    let conversation: Vec<(String, String)> = thread
        .comments
        .iter()
        .map(|c| (c.author.name.clone(), c.body.clone()))
        .collect();

    agent::build_discussion_prompt(
        &thread.file_path,
        &thread.side,
        thread.start_line,
        thread.end_line,
        thread.anchor_content.as_deref(),
        &conversation,
        instruction,
    )
}

fn err400(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
    )
}

fn err500(e: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": e.to_string() })),
    )
}
