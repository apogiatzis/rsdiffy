#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("not a git repository")]
    NotARepo,

    #[error("git command failed: {cmd}\n{stderr}")]
    CommandFailed { cmd: String, stderr: String },

    #[error("invalid git ref: {0}")]
    InvalidRef(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, GitError>;
