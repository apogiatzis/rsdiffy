use std::sync::OnceLock;

use rusqlite::Connection;

use anyhow::Result;

static DB_PATH: OnceLock<String> = OnceLock::new();

/// Initialize the database path. Must be called before `get_db`.
pub fn init_db_path(rsdiffy_dir: &str) {
    let path = format!("{}/reviews.db", rsdiffy_dir);
    let _ = DB_PATH.set(path);
}

/// Open a new connection to the database.
/// Each call returns a fresh connection (no connection pooling needed for SQLite with WAL).
pub fn get_db() -> Result<Connection> {
    let path = DB_PATH
        .get()
        .expect("Database path not initialized. Call init_db_path first.");

    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Open a connection for a specific directory (used during initialization).
pub fn open_db(rsdiffy_dir: &str) -> Result<Connection> {
    let path = format!("{}/reviews.db", rsdiffy_dir);
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    migrate(&conn)?;
    Ok(conn)
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS review_sessions (
            id TEXT PRIMARY KEY,
            ref TEXT NOT NULL,
            head_hash TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS comment_threads (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            file_path TEXT NOT NULL,
            side TEXT NOT NULL DEFAULT 'new',
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            status TEXT NOT NULL DEFAULT 'open',
            anchor_content TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (session_id) REFERENCES review_sessions(id)
        );

        CREATE TABLE IF NOT EXISTS comments (
            id TEXT PRIMARY KEY,
            thread_id TEXT NOT NULL,
            author_name TEXT NOT NULL,
            author_type TEXT NOT NULL DEFAULT 'user',
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (thread_id) REFERENCES comment_threads(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS tours (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            topic TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'building',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (session_id) REFERENCES review_sessions(id)
        );

        CREATE TABLE IF NOT EXISTS tour_steps (
            id TEXT PRIMARY KEY,
            tour_id TEXT NOT NULL,
            sort_order INTEGER NOT NULL,
            file_path TEXT NOT NULL,
            start_line INTEGER NOT NULL,
            end_line INTEGER NOT NULL,
            body TEXT NOT NULL DEFAULT '',
            annotation TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            FOREIGN KEY (tour_id) REFERENCES tours(id) ON DELETE CASCADE
        );

        CREATE INDEX IF NOT EXISTS idx_threads_session ON comment_threads(session_id);
        CREATE INDEX IF NOT EXISTS idx_comments_thread ON comments(thread_id);
        CREATE INDEX IF NOT EXISTS idx_tours_session ON tours(session_id);
        CREATE INDEX IF NOT EXISTS idx_tour_steps_tour ON tour_steps(tour_id);
        ",
    )?;
    Ok(())
}
