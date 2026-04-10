#[derive(Debug, thiserror::Error)]
pub enum GitLabError {
    #[error("GitLab API request failed: {0}")]
    Api(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("GitLab token not found. Set GITLAB_TOKEN or GITLAB_PRIVATE_TOKEN environment variable")]
    TokenNotFound,

    #[error("GitLab remote not detected in git remotes")]
    RemoteNotFound,

    #[error("not a GitLab MR URL: {0}")]
    InvalidMrUrl(String),

    #[error("git command failed: {0}")]
    Git(String),
}

pub type Result<T> = std::result::Result<T, GitLabError>;
