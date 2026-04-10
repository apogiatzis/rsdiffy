pub mod client;
pub mod detection;
pub mod error;
pub mod mr;
pub mod mr_url;

pub use client::GitLabClient;
pub use detection::{detect_remote, is_authenticated, GitLabRemote};
pub use error::{GitLabError, Result};
pub use mr::{
    checkout_mr, fetch_details, get_files, get_mr_base_ref, pull_comments, push_comments,
    MrComment, MrDetails, PulledComment, PulledThread, PushResult,
};
pub use mr_url::{is_gitlab_mr_url, parse_gitlab_mr_url, ParsedMrUrl};
