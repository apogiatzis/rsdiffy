use reqwest::blocking::Client;
use serde::de::DeserializeOwned;

use crate::error::{GitLabError, Result};

/// GitLab API client that handles authentication and base URL.
#[derive(Debug, Clone)]
pub struct GitLabClient {
    base_url: String,
    token: String,
    client: Client,
}

impl GitLabClient {
    /// Create a new client for a GitLab instance.
    /// `base_url` should be like `https://gitlab.com` (no trailing slash).
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: Client::new(),
        }
    }

    /// Create a client using environment variables for auth.
    /// Checks `GITLAB_TOKEN`, then `GITLAB_PRIVATE_TOKEN`.
    pub fn from_env(base_url: &str) -> Result<Self> {
        let token = std::env::var("GITLAB_TOKEN")
            .or_else(|_| std::env::var("GITLAB_PRIVATE_TOKEN"))
            .map_err(|_| GitLabError::TokenNotFound)?;
        Ok(Self::new(base_url, &token))
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// GET a JSON endpoint. `path` should start with `/`.
    pub fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(GitLabError::Api(format!(
                "{} {} -> {} {}",
                "GET", path, status, body
            )));
        }

        Ok(resp.json()?)
    }

    /// POST JSON to an endpoint.
    pub fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .post(&url)
            .header("PRIVATE-TOKEN", &self.token)
            .json(body)
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body_text = resp.text().unwrap_or_default();
            return Err(GitLabError::Api(format!(
                "{} {} -> {} {}",
                "POST", path, status, body_text
            )));
        }

        Ok(resp.json()?)
    }

    /// URL-encode a project path for use in API URLs (e.g. `group/project` -> `group%2Fproject`).
    pub fn encode_project(project_path: &str) -> String {
        project_path.replace('/', "%2F")
    }
}
