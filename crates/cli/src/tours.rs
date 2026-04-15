use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::unescape::unescape_markdown;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TourStep {
    pub id: String,
    pub tour_id: String,
    pub sort_order: i64,
    pub file_path: String,
    pub start_line: i64,
    pub end_line: i64,
    pub body: String,
    pub annotation: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tour {
    pub id: String,
    pub session_id: String,
    pub topic: String,
    pub body: String,
    pub status: String,
    pub created_at: String,
    pub steps: Vec<TourStep>,
}

pub fn create_tour(conn: &Connection, session_id: &str, topic: &str, body: &str) -> Result<Tour> {
    let id = uuid::Uuid::new_v4().to_string();
    let body = unescape_markdown(body);

    conn.execute(
        "INSERT INTO tours (id, session_id, topic, body) VALUES (?1, ?2, ?3, ?4)",
        params![id, session_id, topic, body],
    )?;

    get_tour(conn, &id)
}

pub fn get_tour(conn: &Connection, id: &str) -> Result<Tour> {
    let sql = "SELECT t.id, t.session_id, t.topic, t.body, t.status, t.created_at,
                      s.id as s_id, s.tour_id, s.sort_order, s.file_path, s.start_line, s.end_line, s.body as s_body, s.annotation, s.created_at as s_created_at
               FROM tours t
               LEFT JOIN tour_steps s ON s.tour_id = t.id
               WHERE t.id = ?1
               ORDER BY s.sort_order ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<TourJoinedRow> = stmt
        .query_map(params![id], map_tour_joined)?
        .filter_map(|r| r.ok())
        .collect();

    collapse_tours(rows)
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Tour not found: {}", id))
}

pub fn get_tours_for_session(conn: &Connection, session_id: &str) -> Result<Vec<Tour>> {
    let sql = "SELECT t.id, t.session_id, t.topic, t.body, t.status, t.created_at,
                      s.id as s_id, s.tour_id, s.sort_order, s.file_path, s.start_line, s.end_line, s.body as s_body, s.annotation, s.created_at as s_created_at
               FROM tours t
               LEFT JOIN tour_steps s ON s.tour_id = t.id
               WHERE t.session_id = ?1
               ORDER BY t.created_at ASC, s.sort_order ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<TourJoinedRow> = stmt
        .query_map(params![session_id], map_tour_joined)?
        .filter_map(|r| r.ok())
        .collect();

    Ok(collapse_tours(rows))
}

pub fn add_tour_step(
    conn: &Connection,
    tour_id: &str,
    file_path: &str,
    start_line: i64,
    end_line: i64,
    body: &str,
    annotation: Option<&str>,
) -> Result<TourStep> {
    let id = uuid::Uuid::new_v4().to_string();
    let body = unescape_markdown(body);
    let annotation = annotation.map(unescape_markdown);

    let sort_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM tour_steps WHERE tour_id = ?1",
            params![tour_id],
            |row| row.get(0),
        )
        .unwrap_or(0);

    conn.execute(
        "INSERT INTO tour_steps (id, tour_id, sort_order, file_path, start_line, end_line, body, annotation) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, tour_id, sort_order, file_path, start_line, end_line, body, annotation],
    )?;

    Ok(TourStep {
        id,
        tour_id: tour_id.to_string(),
        sort_order,
        file_path: file_path.to_string(),
        start_line,
        end_line,
        body,
        annotation: annotation.map(|a| a.to_string()),
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

pub fn update_tour_status(conn: &Connection, tour_id: &str, status: &str) -> Result<()> {
    conn.execute(
        "UPDATE tours SET status = ?1 WHERE id = ?2",
        params![status, tour_id],
    )?;
    Ok(())
}

struct TourJoinedRow {
    id: String,
    session_id: String,
    topic: String,
    body: String,
    status: String,
    created_at: String,
    s_id: Option<String>,
    s_tour_id: Option<String>,
    s_sort_order: Option<i64>,
    s_file_path: Option<String>,
    s_start_line: Option<i64>,
    s_end_line: Option<i64>,
    s_body: Option<String>,
    s_annotation: Option<String>,
    s_created_at: Option<String>,
}

fn map_tour_joined(row: &rusqlite::Row) -> rusqlite::Result<TourJoinedRow> {
    Ok(TourJoinedRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        topic: row.get(2)?,
        body: row.get(3)?,
        status: row.get(4)?,
        created_at: row.get(5)?,
        s_id: row.get(6)?,
        s_tour_id: row.get(7)?,
        s_sort_order: row.get(8)?,
        s_file_path: row.get(9)?,
        s_start_line: row.get(10)?,
        s_end_line: row.get(11)?,
        s_body: row.get(12)?,
        s_annotation: row.get(13)?,
        s_created_at: row.get(14)?,
    })
}

fn collapse_tours(rows: Vec<TourJoinedRow>) -> Vec<Tour> {
    let mut tours: Vec<Tour> = Vec::new();
    let mut last_tour_id = String::new();

    for row in rows {
        if row.id != last_tour_id {
            last_tour_id = row.id.clone();
            tours.push(Tour {
                id: row.id.clone(),
                session_id: row.session_id,
                topic: row.topic,
                body: row.body,
                status: row.status,
                created_at: row.created_at,
                steps: Vec::new(),
            });
        }

        if let (Some(s_id), Some(s_tour_id), Some(order), Some(fp), Some(sl), Some(el), Some(sb), Some(sc)) = (
            row.s_id,
            row.s_tour_id,
            row.s_sort_order,
            row.s_file_path,
            row.s_start_line,
            row.s_end_line,
            row.s_body,
            row.s_created_at,
        ) {
            if let Some(tour) = tours.last_mut() {
                tour.steps.push(TourStep {
                    id: s_id,
                    tour_id: s_tour_id,
                    sort_order: order,
                    file_path: fp,
                    start_line: sl,
                    end_line: el,
                    body: sb,
                    annotation: row.s_annotation,
                    created_at: sc,
                });
            }
        }
    }

    tours
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        crate::db::migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO review_sessions (id, ref, head_hash) VALUES ('sess1', 'main', 'abc123')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn create_tour_returns_tour() {
        let conn = setup_db();
        let tour = create_tour(&conn, "sess1", "Auth flow", "How auth works").unwrap();

        assert_eq!(tour.topic, "Auth flow");
        assert_eq!(tour.body, "How auth works");
        assert_eq!(tour.status, "building");
        assert!(tour.steps.is_empty());
    }

    #[test]
    fn get_tour_by_id() {
        let conn = setup_db();
        let created = create_tour(&conn, "sess1", "Test tour", "").unwrap();
        let fetched = get_tour(&conn, &created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.topic, "Test tour");
    }

    #[test]
    fn get_tour_not_found() {
        let conn = setup_db();
        assert!(get_tour(&conn, "nonexistent").is_err());
    }

    #[test]
    fn add_steps_with_auto_ordering() {
        let conn = setup_db();
        let tour = create_tour(&conn, "sess1", "Tour", "").unwrap();

        let s1 = add_tour_step(&conn, &tour.id, "a.rs", 1, 10, "Step 1", None).unwrap();
        let s2 = add_tour_step(&conn, &tour.id, "b.rs", 5, 15, "Step 2", Some("note")).unwrap();

        assert_eq!(s1.sort_order, 0);
        assert_eq!(s2.sort_order, 1);
        assert_eq!(s2.annotation, Some("note".to_string()));

        let fetched = get_tour(&conn, &tour.id).unwrap();
        assert_eq!(fetched.steps.len(), 2);
        assert_eq!(fetched.steps[0].file_path, "a.rs");
        assert_eq!(fetched.steps[1].file_path, "b.rs");
    }

    #[test]
    fn update_tour_status_changes_status() {
        let conn = setup_db();
        let tour = create_tour(&conn, "sess1", "Tour", "").unwrap();
        assert_eq!(tour.status, "building");

        update_tour_status(&conn, &tour.id, "complete").unwrap();
        let fetched = get_tour(&conn, &tour.id).unwrap();
        assert_eq!(fetched.status, "complete");
    }

    #[test]
    fn get_tours_for_session_returns_all() {
        let conn = setup_db();
        create_tour(&conn, "sess1", "Tour A", "").unwrap();
        create_tour(&conn, "sess1", "Tour B", "").unwrap();

        let tours = get_tours_for_session(&conn, "sess1").unwrap();
        assert_eq!(tours.len(), 2);
    }

    #[test]
    fn get_tours_for_session_empty() {
        let conn = setup_db();
        let tours = get_tours_for_session(&conn, "sess1").unwrap();
        assert!(tours.is_empty());
    }
}
