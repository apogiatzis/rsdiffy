use regex::Regex;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use crate::error::{GitLabError, Result};

/// Detected GitLab remote information.
#[derive(Debug, Clone, PartialEq)]
pub struct GitLabRemote {
    /// The GitLab instance base URL (e.g. `https://gitlab.com`)
    pub base_url: String,
    /// The project path (e.g. `group/subgroup/project`)
    pub project_path: String,
}

static SSH_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^git@([^:]+):(.+?)(?:\.git)?$").unwrap());

static HTTPS_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https?://([^/]+)/(.+?)(?:\.git)?$").unwrap());

static SSH_PROTO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ssh://[^@]+@([^:/]+)(?::\d+)?/(.+?)(?:\.git)?$").unwrap());

/// Parse a git remote URL into host and project path.
fn parse_remote_url(url: &str) -> Option<(String, String)> {
    if let Some(caps) = SSH_RE.captures(url) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }
    if let Some(caps) = HTTPS_RE.captures(url) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }
    if let Some(caps) = SSH_PROTO_RE.captures(url) {
        return Some((caps[1].to_string(), caps[2].to_string()));
    }
    None
}

/// Detect a GitLab remote from the current repo's git remotes.
///
/// Detection strategy:
/// 1. If `GITLAB_HOST` env var is set, look for remotes matching that host
/// 2. Otherwise, look for remotes with "gitlab" in the hostname
/// 3. Checks `origin` first, then all other remotes
pub fn detect_remote() -> Result<GitLabRemote> {
    let output = Command::new("git")
        .args(["remote", "-v"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| GitLabError::Git(e.to_string()))?;

    if !output.status.success() {
        return Err(GitLabError::RemoteNotFound);
    }

    let remotes_text = String::from_utf8_lossy(&output.stdout);
    let gitlab_host = std::env::var("GITLAB_HOST").ok();

    let remotes: Vec<(&str, &str)> = remotes_text
        .lines()
        .filter(|line| line.ends_with("(fetch)"))
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let name = parts.next()?;
            let url = parts.next()?;
            Some((name, url))
        })
        .collect();

    let mut sorted = remotes.clone();
    sorted.sort_by_key(|(name, _)| if *name == "origin" { 0 } else { 1 });

    for (_, url) in sorted {
        if let Some((host, project_path)) = parse_remote_url(url) {
            let is_gitlab = match &gitlab_host {
                Some(configured_host) => host == *configured_host,
                None => host.contains("gitlab"),
            };

            if is_gitlab {
                let scheme = if url.starts_with("http://") {
                    "http"
                } else {
                    "https"
                };
                return Ok(GitLabRemote {
                    base_url: format!("{}://{}", scheme, host),
                    project_path,
                });
            }
        }
    }

    Err(GitLabError::RemoteNotFound)
}

/// Check if a GitLab token is available in the environment.
pub fn is_authenticated() -> bool {
    std::env::var("GITLAB_TOKEN").is_ok() || std::env::var("GITLAB_PRIVATE_TOKEN").is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_remote() {
        let (host, path) = parse_remote_url("https://gitlab.com/mygroup/myproject.git").unwrap();
        assert_eq!(host, "gitlab.com");
        assert_eq!(path, "mygroup/myproject");
    }

    #[test]
    fn parses_https_remote_without_git_suffix() {
        let (host, path) = parse_remote_url("https://gitlab.com/mygroup/myproject").unwrap();
        assert_eq!(host, "gitlab.com");
        assert_eq!(path, "mygroup/myproject");
    }

    #[test]
    fn parses_ssh_remote() {
        let (host, path) = parse_remote_url("git@gitlab.com:mygroup/myproject.git").unwrap();
        assert_eq!(host, "gitlab.com");
        assert_eq!(path, "mygroup/myproject");
    }

    #[test]
    fn parses_ssh_remote_without_git_suffix() {
        let (host, path) = parse_remote_url("git@gitlab.com:mygroup/myproject").unwrap();
        assert_eq!(host, "gitlab.com");
        assert_eq!(path, "mygroup/myproject");
    }

    #[test]
    fn parses_nested_group_path() {
        let (host, path) =
            parse_remote_url("https://gitlab.com/group/subgroup/project.git").unwrap();
        assert_eq!(host, "gitlab.com");
        assert_eq!(path, "group/subgroup/project");
    }

    #[test]
    fn parses_self_hosted_https() {
        let (host, path) =
            parse_remote_url("https://gitlab.company.com/team/repo.git").unwrap();
        assert_eq!(host, "gitlab.company.com");
        assert_eq!(path, "team/repo");
    }

    #[test]
    fn parses_self_hosted_ssh() {
        let (host, path) =
            parse_remote_url("git@git.internal.io:infra/deploy-tools.git").unwrap();
        assert_eq!(host, "git.internal.io");
        assert_eq!(path, "infra/deploy-tools");
    }

    #[test]
    fn parses_ssh_with_protocol_and_port() {
        let (host, path) =
            parse_remote_url("ssh://git@gitlab.example.com:2222/team/project.git").unwrap();
        assert_eq!(host, "gitlab.example.com");
        assert_eq!(path, "team/project");
    }

    #[test]
    fn returns_none_for_invalid_url() {
        assert!(parse_remote_url("not-a-url").is_none());
        assert!(parse_remote_url("").is_none());
    }
}
