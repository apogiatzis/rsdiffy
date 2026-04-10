use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::process::{Command, Stdio};

use crate::client::GitLabClient;
use crate::error::{GitLabError, Result};

#[derive(Debug, Clone, Deserialize)]
struct MrResponse {
    iid: u64,
    title: String,
    web_url: String,
    created_at: String,
    sha: String,
    source_branch: String,
    target_branch: String,
    user_notes_count: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct DiffFileResponse {
    new_path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct DiscussionResponse {
    #[allow(dead_code)]
    id: String,
    notes: Vec<NoteResponse>,
}

#[derive(Debug, Clone, Deserialize)]
struct NoteResponse {
    #[allow(dead_code)]
    id: u64,
    body: String,
    #[serde(default)]
    system: bool,
    author: NoteAuthor,
    created_at: String,
    #[serde(default)]
    position: Option<NotePosition>,
}

#[derive(Debug, Clone, Deserialize)]
struct NoteAuthor {
    username: String,
}

#[derive(Debug, Clone, Deserialize)]
struct NotePosition {
    new_path: Option<String>,
    old_path: Option<String>,
    new_line: Option<u32>,
    old_line: Option<u32>,
    position_type: Option<String>,
}

/// Details about a GitLab merge request.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MrDetails {
    pub mr_iid: u64,
    pub title: String,
    pub url: String,
    pub created_at: String,
    pub head_sha: String,
    pub source_branch: String,
    pub target_branch: String,
    pub comment_count: u64,
}

/// A comment to push to a GitLab MR.
#[derive(Debug, Clone)]
pub struct MrComment {
    pub file_path: String,
    /// `old` or `new`
    pub side: String,
    pub start_line: Option<u32>,
    pub end_line: u32,
    pub body: String,
}

/// Result of pushing comments to GitLab.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub pushed: u32,
    pub skipped: u32,
    pub failed: u32,
    pub errors: Vec<String>,
}

/// A comment thread pulled from GitLab.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PulledThread {
    pub file_path: String,
    pub side: String,
    pub start_line: Option<u32>,
    pub end_line: u32,
    pub comments: Vec<PulledComment>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PulledComment {
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Fetch MR details.
pub fn fetch_details(
    client: &GitLabClient,
    project_path: &str,
    mr_iid: u64,
) -> Result<MrDetails> {
    let encoded = GitLabClient::encode_project(project_path);
    let path = format!("/api/v4/projects/{}/merge_requests/{}", encoded, mr_iid);
    let mr: MrResponse = client.get(&path)?;

    Ok(MrDetails {
        mr_iid: mr.iid,
        title: mr.title,
        url: mr.web_url,
        created_at: mr.created_at,
        head_sha: mr.sha,
        source_branch: mr.source_branch,
        target_branch: mr.target_branch,
        comment_count: mr.user_notes_count,
    })
}

/// Get the set of files changed in an MR.
pub fn get_files(
    client: &GitLabClient,
    project_path: &str,
    mr_iid: u64,
) -> Result<HashSet<String>> {
    let encoded = GitLabClient::encode_project(project_path);
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/diffs",
        encoded, mr_iid
    );
    let diffs: Vec<DiffFileResponse> = client.get(&path)?;
    Ok(diffs.into_iter().map(|d| d.new_path).collect())
}

/// Pull existing MR discussion threads (non-system notes only).
pub fn pull_comments(
    client: &GitLabClient,
    project_path: &str,
    mr_iid: u64,
) -> Result<Vec<PulledThread>> {
    let encoded = GitLabClient::encode_project(project_path);
    let path = format!(
        "/api/v4/projects/{}/merge_requests/{}/discussions",
        encoded, mr_iid
    );
    let discussions: Vec<DiscussionResponse> = client.get(&path)?;

    let mut threads = Vec::new();
    for disc in discussions {
        let notes: Vec<&NoteResponse> = disc.notes.iter().filter(|n| !n.system).collect();
        if notes.is_empty() {
            continue;
        }

        // Use the first note's position for file/line info
        let first = &notes[0];
        let (file_path, side, end_line) = match &first.position {
            Some(pos) if pos.position_type.as_deref() == Some("text") => {
                let fp = pos
                    .new_path
                    .as_deref()
                    .or(pos.old_path.as_deref())
                    .unwrap_or("__general__")
                    .to_string();
                let (s, line) = if let Some(new_line) = pos.new_line {
                    ("new".to_string(), new_line)
                } else if let Some(old_line) = pos.old_line {
                    ("old".to_string(), old_line)
                } else {
                    ("new".to_string(), 0)
                };
                (fp, s, line)
            }
            _ => ("__general__".to_string(), "new".to_string(), 0),
        };

        let comments: Vec<PulledComment> = notes
            .iter()
            .map(|n| PulledComment {
                author: n.author.username.clone(),
                body: n.body.clone(),
                created_at: n.created_at.clone(),
            })
            .collect();

        threads.push(PulledThread {
            file_path,
            side,
            start_line: None,
            end_line,
            comments,
        });
    }

    Ok(threads)
}

/// Push local review comments as new MR discussions.
pub fn push_comments(
    client: &GitLabClient,
    project_path: &str,
    mr_iid: u64,
    head_sha: &str,
    comments: &[MrComment],
) -> PushResult {
    let encoded = GitLabClient::encode_project(project_path);
    let base_path = format!(
        "/api/v4/projects/{}/merge_requests/{}/discussions",
        encoded, mr_iid
    );

    let existing = pull_comments(client, project_path, mr_iid).unwrap_or_default();
    let existing_keys: HashSet<(String, u32, String)> = existing
        .iter()
        .flat_map(|t| {
            t.comments.iter().map(|c| {
                (t.file_path.clone(), t.end_line, c.body.clone())
            })
        })
        .collect();

    let mut result = PushResult {
        pushed: 0,
        skipped: 0,
        failed: 0,
        errors: Vec::new(),
    };

    for comment in comments {
        let key = (
            comment.file_path.clone(),
            comment.end_line,
            comment.body.clone(),
        );
        if existing_keys.contains(&key) {
            result.skipped += 1;
            continue;
        }

        let body = if comment.file_path == "__general__" {
            serde_json::json!({
                "body": comment.body,
            })
        } else {
            let position = serde_json::json!({
                "position_type": "text",
                "base_sha": head_sha,
                "head_sha": head_sha,
                "start_sha": head_sha,
                "new_path": comment.file_path,
                "old_path": comment.file_path,
                "new_line": comment.end_line,
            });

            serde_json::json!({
                "body": comment.body,
                "position": position,
            })
        };

        match client.post::<serde_json::Value>(&base_path, &body) {
            Ok(_) => result.pushed += 1,
            Err(e) => {
                result.failed += 1;
                result.errors.push(e.to_string());
            }
        }
    }

    result
}

/// Get the target branch ref for an MR.
pub fn get_mr_base_ref(
    client: &GitLabClient,
    project_path: &str,
    mr_iid: u64,
) -> Result<String> {
    let details = fetch_details(client, project_path, mr_iid)?;
    Ok(details.target_branch)
}

/// Check out an MR's source branch locally.
pub fn checkout_mr(mr_iid: u64, source_branch: &str) -> Result<()> {
    let status = Command::new("git")
        .args([
            "fetch",
            "origin",
            &format!("merge-requests/{}/head:{}", mr_iid, source_branch),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| GitLabError::Git(e.to_string()))?;

    if !status.success() {
        return Err(GitLabError::Git(format!(
            "failed to fetch MR !{} ref",
            mr_iid
        )));
    }

    let status = Command::new("git")
        .args(["checkout", source_branch])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| GitLabError::Git(e.to_string()))?;

    if !status.success() {
        return Err(GitLabError::Git(format!(
            "failed to checkout branch {}",
            source_branch
        )));
    }

    Ok(())
}
