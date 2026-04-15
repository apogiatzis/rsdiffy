use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;

use crate::unescape::unescape_markdown;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadAuthor {
    pub name: String,
    #[serde(rename = "type")]
    pub author_type: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadComment {
    pub id: String,
    pub author: ThreadAuthor,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub session_id: String,
    pub file_path: String,
    pub side: String,
    pub start_line: i64,
    pub end_line: i64,
    pub status: String,
    pub anchor_content: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub comments: Vec<ThreadComment>,
}

#[allow(clippy::too_many_arguments)]
pub fn create_thread(
    conn: &Connection,
    session_id: &str,
    file_path: &str,
    side: &str,
    start_line: i64,
    end_line: i64,
    body: &str,
    author_name: &str,
    author_type: &str,
    anchor_content: Option<&str>,
) -> Result<Thread> {
    let thread_id = uuid::Uuid::new_v4().to_string();
    let comment_id = uuid::Uuid::new_v4().to_string();
    let body = unescape_markdown(body);

    conn.execute(
        "INSERT INTO comment_threads (id, session_id, file_path, side, start_line, end_line, anchor_content) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![thread_id, session_id, file_path, side, start_line, end_line, anchor_content],
    )?;

    conn.execute(
        "INSERT INTO comments (id, thread_id, author_name, author_type, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![comment_id, thread_id, author_name, author_type, body],
    )?;

    get_thread(conn, &thread_id)
}

pub fn get_threads_for_session(
    conn: &Connection,
    session_id: &str,
    status: Option<&str>,
) -> Result<Vec<Thread>> {
    let mut sql = String::from(
        "SELECT t.id, t.session_id, t.file_path, t.side, t.start_line, t.end_line, t.status, t.anchor_content, t.created_at, t.updated_at,
                c.id as c_id, c.author_name, c.author_type, c.body, c.created_at as c_created_at
         FROM comment_threads t
         LEFT JOIN comments c ON c.thread_id = t.id
         WHERE t.session_id = ?1",
    );

    if status.is_some() {
        sql.push_str(" AND t.status = ?2");
    }
    sql.push_str(" ORDER BY t.created_at ASC, c.created_at ASC");

    let mut stmt = conn.prepare(&sql)?;
    let rows: Vec<JoinedRow> = if let Some(status) = status {
        stmt.query_map(params![session_id, status], map_joined_row)?
            .filter_map(|r| r.ok())
            .collect()
    } else {
        stmt.query_map(params![session_id], map_joined_row)?
            .filter_map(|r| r.ok())
            .collect()
    };

    Ok(collapse_threads(rows))
}

pub fn get_thread(conn: &Connection, id: &str) -> Result<Thread> {
    // Support prefix matching (8+ chars)
    let pattern = if id.len() < 36 {
        format!("{}%", id)
    } else {
        id.to_string()
    };

    let sql = "SELECT t.id, t.session_id, t.file_path, t.side, t.start_line, t.end_line, t.status, t.anchor_content, t.created_at, t.updated_at,
                      c.id as c_id, c.author_name, c.author_type, c.body, c.created_at as c_created_at
               FROM comment_threads t
               LEFT JOIN comments c ON c.thread_id = t.id
               WHERE t.id LIKE ?1
               ORDER BY c.created_at ASC";

    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<JoinedRow> = stmt
        .query_map(params![pattern], map_joined_row)?
        .filter_map(|r| r.ok())
        .collect();

    let threads = collapse_threads(rows);
    threads
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("Thread not found: {}", id))
}

pub fn add_reply(
    conn: &Connection,
    thread_id: &str,
    body: &str,
    author_name: &str,
    author_type: &str,
) -> Result<ThreadComment> {
    let comment_id = uuid::Uuid::new_v4().to_string();
    let body = unescape_markdown(body);

    conn.execute(
        "INSERT INTO comments (id, thread_id, author_name, author_type, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![comment_id, thread_id, author_name, author_type, body],
    )?;

    // If user replies, reopen the thread
    if author_type == "user" {
        conn.execute(
            "UPDATE comment_threads SET status = 'open', updated_at = datetime('now') WHERE id = ?1",
            params![thread_id],
        )?;
    }

    conn.execute(
        "UPDATE comment_threads SET updated_at = datetime('now') WHERE id = ?1",
        params![thread_id],
    )?;

    Ok(ThreadComment {
        id: comment_id,
        author: ThreadAuthor {
            name: author_name.to_string(),
            author_type: author_type.to_string(),
        },
        body,
        created_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

pub fn update_thread_status(
    conn: &Connection,
    thread_id: &str,
    status: &str,
    summary_body: Option<&str>,
    summary_author_name: Option<&str>,
    summary_author_type: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE comment_threads SET status = ?1, updated_at = datetime('now') WHERE id = ?2",
        params![status, thread_id],
    )?;

    if let (Some(body), Some(name), Some(atype)) =
        (summary_body, summary_author_name, summary_author_type)
    {
        let comment_id = uuid::Uuid::new_v4().to_string();
        let body = unescape_markdown(body);
        conn.execute(
            "INSERT INTO comments (id, thread_id, author_name, author_type, body) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![comment_id, thread_id, name, atype, body],
        )?;
    }

    Ok(())
}

pub fn delete_thread(conn: &Connection, thread_id: &str) -> Result<()> {
    conn.execute("DELETE FROM comments WHERE thread_id = ?1", params![thread_id])?;
    conn.execute("DELETE FROM comment_threads WHERE id = ?1", params![thread_id])?;
    Ok(())
}

pub fn delete_all_threads_for_session(conn: &Connection, session_id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM comments WHERE thread_id IN (SELECT id FROM comment_threads WHERE session_id = ?1)",
        params![session_id],
    )?;
    conn.execute(
        "DELETE FROM comment_threads WHERE session_id = ?1",
        params![session_id],
    )?;
    Ok(())
}

pub fn edit_comment(conn: &Connection, comment_id: &str, body: &str) -> Result<()> {
    let body = unescape_markdown(body);
    conn.execute(
        "UPDATE comments SET body = ?1 WHERE id = ?2",
        params![body, comment_id],
    )?;
    Ok(())
}

pub fn delete_comment(conn: &Connection, comment_id: &str) -> Result<()> {
    let thread_id: String = conn.query_row(
        "SELECT thread_id FROM comments WHERE id = ?1",
        params![comment_id],
        |row| row.get(0),
    )?;

    conn.execute("DELETE FROM comments WHERE id = ?1", params![comment_id])?;

    let remaining: i64 = conn.query_row(
        "SELECT COUNT(*) FROM comments WHERE thread_id = ?1",
        params![thread_id],
        |row| row.get(0),
    )?;

    if remaining == 0 {
        conn.execute(
            "DELETE FROM comment_threads WHERE id = ?1",
            params![thread_id],
        )?;
    }

    Ok(())
}

struct JoinedRow {
    id: String,
    session_id: String,
    file_path: String,
    side: String,
    start_line: i64,
    end_line: i64,
    status: String,
    anchor_content: Option<String>,
    created_at: String,
    updated_at: String,
    c_id: Option<String>,
    c_author_name: Option<String>,
    c_author_type: Option<String>,
    c_body: Option<String>,
    c_created_at: Option<String>,
}

fn map_joined_row(row: &rusqlite::Row) -> rusqlite::Result<JoinedRow> {
    Ok(JoinedRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        file_path: row.get(2)?,
        side: row.get(3)?,
        start_line: row.get(4)?,
        end_line: row.get(5)?,
        status: row.get(6)?,
        anchor_content: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        c_id: row.get(10)?,
        c_author_name: row.get(11)?,
        c_author_type: row.get(12)?,
        c_body: row.get(13)?,
        c_created_at: row.get(14)?,
    })
}

fn collapse_threads(rows: Vec<JoinedRow>) -> Vec<Thread> {
    let mut threads: Vec<Thread> = Vec::new();
    let mut last_thread_id = String::new();

    for row in rows {
        if row.id != last_thread_id {
            last_thread_id = row.id.clone();
            threads.push(Thread {
                id: row.id.clone(),
                session_id: row.session_id,
                file_path: row.file_path,
                side: row.side,
                start_line: row.start_line,
                end_line: row.end_line,
                status: row.status,
                anchor_content: row.anchor_content,
                created_at: row.created_at,
                updated_at: row.updated_at,
                comments: Vec::new(),
            });
        }

        if let (Some(c_id), Some(name), Some(atype), Some(body), Some(cat)) = (
            row.c_id,
            row.c_author_name,
            row.c_author_type,
            row.c_body,
            row.c_created_at,
        ) {
            if let Some(thread) = threads.last_mut() {
                thread.comments.push(ThreadComment {
                    id: c_id,
                    author: ThreadAuthor {
                        name,
                        author_type: atype,
                    },
                    body,
                    created_at: cat,
                });
            }
        }
    }

    threads
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
    fn create_thread_returns_thread_with_comment() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "src/main.rs", "new", 10, 20, "Bug here", "alice", "user", None,
        )
        .unwrap();

        assert_eq!(thread.file_path, "src/main.rs");
        assert_eq!(thread.side, "new");
        assert_eq!(thread.start_line, 10);
        assert_eq!(thread.end_line, 20);
        assert_eq!(thread.status, "open");
        assert_eq!(thread.comments.len(), 1);
        assert_eq!(thread.comments[0].body, "Bug here");
        assert_eq!(thread.comments[0].author.name, "alice");
    }

    #[test]
    fn create_thread_with_anchor_content() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "lib.rs", "old", 1, 5, "Review this", "bot", "bot",
            Some("fn main() {}"),
        )
        .unwrap();

        assert_eq!(thread.anchor_content, Some("fn main() {}".to_string()));
    }

    #[test]
    fn get_thread_by_full_id() {
        let conn = setup_db();
        let created = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "test", "user", "user", None,
        )
        .unwrap();

        let fetched = get_thread(&conn, &created.id).unwrap();
        assert_eq!(fetched.id, created.id);
        assert_eq!(fetched.comments.len(), 1);
    }

    #[test]
    fn get_thread_by_prefix() {
        let conn = setup_db();
        let created = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "test", "user", "user", None,
        )
        .unwrap();

        let prefix = &created.id[..8];
        let fetched = get_thread(&conn, prefix).unwrap();
        assert_eq!(fetched.id, created.id);
    }

    #[test]
    fn get_thread_not_found() {
        let conn = setup_db();
        assert!(get_thread(&conn, "nonexistent-id-that-does-not-exist").is_err());
    }

    #[test]
    fn add_reply_creates_comment() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "initial", "alice", "user", None,
        )
        .unwrap();

        let reply = add_reply(&conn, &thread.id, "reply body", "bob", "user").unwrap();
        assert_eq!(reply.body, "reply body");
        assert_eq!(reply.author.name, "bob");

        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.comments.len(), 2);
    }

    #[test]
    fn user_reply_reopens_resolved_thread() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "issue", "alice", "user", None,
        )
        .unwrap();

        update_thread_status(&conn, &thread.id, "resolved", None, None, None).unwrap();
        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.status, "resolved");

        add_reply(&conn, &thread.id, "wait, not fixed", "alice", "user").unwrap();
        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.status, "open");
    }

    #[test]
    fn bot_reply_does_not_reopen_thread() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "issue", "alice", "user", None,
        )
        .unwrap();

        update_thread_status(&conn, &thread.id, "resolved", None, None, None).unwrap();
        add_reply(&conn, &thread.id, "agent response", "claude", "bot").unwrap();

        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.status, "resolved");
    }

    #[test]
    fn get_threads_for_session_returns_all() {
        let conn = setup_db();
        create_thread(&conn, "sess1", "a.rs", "new", 1, 1, "first", "u", "user", None).unwrap();
        create_thread(&conn, "sess1", "b.rs", "new", 5, 5, "second", "u", "user", None).unwrap();

        let threads = get_threads_for_session(&conn, "sess1", None).unwrap();
        assert_eq!(threads.len(), 2);
    }

    #[test]
    fn get_threads_for_session_filters_by_status() {
        let conn = setup_db();
        let t1 = create_thread(&conn, "sess1", "a.rs", "new", 1, 1, "open one", "u", "user", None).unwrap();
        create_thread(&conn, "sess1", "b.rs", "new", 5, 5, "open two", "u", "user", None).unwrap();
        update_thread_status(&conn, &t1.id, "resolved", None, None, None).unwrap();

        let open = get_threads_for_session(&conn, "sess1", Some("open")).unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].file_path, "b.rs");

        let resolved = get_threads_for_session(&conn, "sess1", Some("resolved")).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].file_path, "a.rs");
    }

    #[test]
    fn update_thread_status_with_summary() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "issue", "alice", "user", None,
        )
        .unwrap();

        update_thread_status(
            &conn, &thread.id, "resolved", Some("Fixed it"), Some("system"), Some("bot"),
        )
        .unwrap();

        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.status, "resolved");
        assert_eq!(fetched.comments.len(), 2);
        assert_eq!(fetched.comments[1].body, "Fixed it");
    }

    #[test]
    fn delete_thread_removes_thread_and_comments() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "test", "u", "user", None,
        )
        .unwrap();
        add_reply(&conn, &thread.id, "reply", "u", "user").unwrap();

        delete_thread(&conn, &thread.id).unwrap();
        assert!(get_thread(&conn, &thread.id).is_err());
    }

    #[test]
    fn delete_all_threads_for_session_clears_everything() {
        let conn = setup_db();
        create_thread(&conn, "sess1", "a.rs", "new", 1, 1, "first", "u", "user", None).unwrap();
        create_thread(&conn, "sess1", "b.rs", "new", 2, 2, "second", "u", "user", None).unwrap();

        delete_all_threads_for_session(&conn, "sess1").unwrap();
        let threads = get_threads_for_session(&conn, "sess1", None).unwrap();
        assert!(threads.is_empty());
    }

    #[test]
    fn edit_comment_updates_body() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "original", "u", "user", None,
        )
        .unwrap();
        let comment_id = &thread.comments[0].id;

        edit_comment(&conn, comment_id, "updated body").unwrap();

        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.comments[0].body, "updated body");
    }

    #[test]
    fn delete_last_comment_deletes_thread() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "only comment", "u", "user", None,
        )
        .unwrap();
        let comment_id = thread.comments[0].id.clone();

        delete_comment(&conn, &comment_id).unwrap();
        assert!(get_thread(&conn, &thread.id).is_err());
    }

    #[test]
    fn delete_one_of_many_comments_keeps_thread() {
        let conn = setup_db();
        let thread = create_thread(
            &conn, "sess1", "a.rs", "new", 1, 1, "first", "u", "user", None,
        )
        .unwrap();
        let reply = add_reply(&conn, &thread.id, "second", "u", "user").unwrap();

        delete_comment(&conn, &reply.id).unwrap();

        let fetched = get_thread(&conn, &thread.id).unwrap();
        assert_eq!(fetched.comments.len(), 1);
        assert_eq!(fetched.comments[0].body, "first");
    }
}
