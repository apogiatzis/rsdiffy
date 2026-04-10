use std::fs;
use std::path::Path;

use anyhow::Result;
use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::db;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    #[serde(rename = "ref")]
    pub git_ref: String,
    pub head_hash: String,
}

/// Find an existing session or create a new one for the given ref.
pub fn find_or_create_session(
    rsdiffy_dir: &str,
    git_ref: &str,
    head_hash: &str,
) -> Result<Session> {
    let conn = db::open_db(rsdiffy_dir)?;

    let existing: Option<Session> = conn
        .query_row(
            "SELECT id, ref, head_hash FROM review_sessions WHERE ref = ?1 AND head_hash = ?2",
            params![git_ref, head_hash],
            |row| {
                Ok(Session {
                    id: row.get(0)?,
                    git_ref: row.get(1)?,
                    head_hash: row.get(2)?,
                })
            },
        )
        .ok();

    if let Some(session) = existing {
        save_current_session(rsdiffy_dir, &session)?;
        return Ok(session);
    }

    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO review_sessions (id, ref, head_hash) VALUES (?1, ?2, ?3)",
        params![id, git_ref, head_hash],
    )?;

    let session = Session {
        id,
        git_ref: git_ref.to_string(),
        head_hash: head_hash.to_string(),
    };

    save_current_session(rsdiffy_dir, &session)?;
    Ok(session)
}

/// Read the current session from the cached file.
pub fn get_current_session(rsdiffy_dir: &str) -> Result<Option<Session>> {
    let path = Path::new(rsdiffy_dir).join("current-session");
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    let session: Session = serde_json::from_str(&content)?;
    Ok(Some(session))
}

fn save_current_session(rsdiffy_dir: &str, session: &Session) -> Result<()> {
    let path = Path::new(rsdiffy_dir).join("current-session");
    fs::write(path, serde_json::to_string(session)?)?;
    Ok(())
}
