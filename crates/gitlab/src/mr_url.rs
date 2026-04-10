use regex::Regex;
use std::sync::LazyLock;

use crate::error::{GitLabError, Result};

/// Parsed GitLab MR URL.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedMrUrl {
    /// The GitLab instance base URL (e.g. `https://gitlab.com`)
    pub base_url: String,
    /// The project path (e.g. `group/project`)
    pub project_path: String,
    /// The merge request IID (project-scoped ID)
    pub mr_iid: u64,
}

// Matches: https://gitlab.com/group/project/-/merge_requests/123
// Also: https://gitlab.company.com/group/subgroup/project/-/merge_requests/456
static MR_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(https?://[^/]+)/(.+?)/-/merge_requests/(\d+)").unwrap()
});

/// Check if a URL looks like a GitLab MR URL.
pub fn is_gitlab_mr_url(url: &str) -> bool {
    MR_URL_RE.is_match(url)
}

/// Parse a GitLab MR URL into its components.
pub fn parse_gitlab_mr_url(url: &str) -> Result<ParsedMrUrl> {
    let caps = MR_URL_RE
        .captures(url)
        .ok_or_else(|| GitLabError::InvalidMrUrl(url.to_string()))?;

    Ok(ParsedMrUrl {
        base_url: caps[1].to_string(),
        project_path: caps[2].to_string(),
        mr_iid: caps[3].parse().unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_mr_url() {
        let parsed =
            parse_gitlab_mr_url("https://gitlab.com/mygroup/myproject/-/merge_requests/42")
                .unwrap();
        assert_eq!(parsed.base_url, "https://gitlab.com");
        assert_eq!(parsed.project_path, "mygroup/myproject");
        assert_eq!(parsed.mr_iid, 42);
    }

    #[test]
    fn parses_nested_group_mr_url() {
        let parsed = parse_gitlab_mr_url(
            "https://gitlab.com/group/subgroup/project/-/merge_requests/100",
        )
        .unwrap();
        assert_eq!(parsed.base_url, "https://gitlab.com");
        assert_eq!(parsed.project_path, "group/subgroup/project");
        assert_eq!(parsed.mr_iid, 100);
    }

    #[test]
    fn parses_self_hosted_mr_url() {
        let parsed = parse_gitlab_mr_url(
            "https://gitlab.company.com/team/repo/-/merge_requests/7",
        )
        .unwrap();
        assert_eq!(parsed.base_url, "https://gitlab.company.com");
        assert_eq!(parsed.project_path, "team/repo");
        assert_eq!(parsed.mr_iid, 7);
    }

    #[test]
    fn parses_http_mr_url() {
        let parsed =
            parse_gitlab_mr_url("http://gitlab.local/ops/infra/-/merge_requests/3").unwrap();
        assert_eq!(parsed.base_url, "http://gitlab.local");
        assert_eq!(parsed.project_path, "ops/infra");
        assert_eq!(parsed.mr_iid, 3);
    }

    #[test]
    fn detects_valid_mr_urls() {
        assert!(is_gitlab_mr_url(
            "https://gitlab.com/group/project/-/merge_requests/1"
        ));
        assert!(is_gitlab_mr_url(
            "https://git.internal.io/a/b/c/-/merge_requests/99"
        ));
    }

    #[test]
    fn rejects_non_mr_urls() {
        assert!(!is_gitlab_mr_url("https://gitlab.com/group/project"));
        assert!(!is_gitlab_mr_url(
            "https://github.com/owner/repo/pull/123"
        ));
        assert!(!is_gitlab_mr_url("not a url"));
        assert!(!is_gitlab_mr_url(""));
    }

    #[test]
    fn parse_fails_for_invalid_url() {
        assert!(parse_gitlab_mr_url("https://gitlab.com/group/project").is_err());
    }
}
