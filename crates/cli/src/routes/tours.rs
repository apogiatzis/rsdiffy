use std::sync::Arc;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::Json;
use axum::routing::{get, post};
use axum::{extract::Query, Router};
use serde::Deserialize;

use crate::db;
use crate::server::AppState;
use crate::tours;

pub fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/tours", get(get_tours).post(create_tour))
        .route("/api/tours/{id}", get(get_tour).patch(update_tour))
        .route("/api/tours/{id}/steps", post(add_step))
}

#[derive(Deserialize)]
struct ToursQuery {
    session: Option<String>,
}

async fn get_tours(
    Query(q): Query<ToursQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sid = q.session.as_deref().ok_or_else(|| err400("Missing session parameter"))?;
    let conn = db::get_db().map_err(err500)?;
    let result = tours::get_tours_for_session(&conn, sid).map_err(err500)?;
    Ok(Json(serde_json::to_value(result).unwrap()))
}

#[derive(Deserialize)]
struct CreateTourBody {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    topic: Option<String>,
    body: Option<String>,
}

async fn create_tour(
    Json(body): Json<CreateTourBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let sid = body.session_id.as_deref().ok_or_else(|| err400("Missing sessionId"))?;
    let topic = body.topic.as_deref().ok_or_else(|| err400("Missing topic"))?;
    let tour_body = body.body.as_deref().unwrap_or("");

    let conn = db::get_db().map_err(err500)?;
    let tour = tours::create_tour(&conn, sid, topic, tour_body).map_err(err500)?;
    Ok(Json(serde_json::to_value(tour).unwrap()))
}

async fn get_tour(
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let conn = db::get_db().map_err(err500)?;
    let tour = tours::get_tour(&conn, &id).map_err(|_| err404("Tour not found"))?;
    Ok(Json(serde_json::to_value(tour).unwrap()))
}

#[derive(Deserialize)]
struct UpdateTourBody {
    status: Option<String>,
}

async fn update_tour(
    Path(id): Path<String>,
    Json(body): Json<UpdateTourBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let status = body.status.as_deref().ok_or_else(|| err400("Missing status"))?;
    let conn = db::get_db().map_err(err500)?;
    tours::update_tour_status(&conn, &id, status).map_err(err500)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
struct AddStepBody {
    #[serde(rename = "filePath")]
    file_path: Option<String>,
    #[serde(rename = "startLine")]
    start_line: Option<i64>,
    #[serde(rename = "endLine")]
    end_line: Option<i64>,
    body: Option<String>,
    annotation: Option<String>,
}

async fn add_step(
    Path(tour_id): Path<String>,
    Json(body): Json<AddStepBody>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let file_path = body.file_path.as_deref().ok_or_else(|| err400("Missing filePath"))?;
    let start_line = body.start_line.ok_or_else(|| err400("Missing startLine"))?;
    let end_line = body.end_line.ok_or_else(|| err400("Missing endLine"))?;
    let step_body = body.body.as_deref().unwrap_or("");

    let conn = db::get_db().map_err(err500)?;
    let step = tours::add_tour_step(
        &conn,
        &tour_id,
        file_path,
        start_line,
        end_line,
        step_body,
        body.annotation.as_deref(),
    )
    .map_err(err500)?;

    Ok(Json(serde_json::to_value(step).unwrap()))
}

fn err400(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": msg })),
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
