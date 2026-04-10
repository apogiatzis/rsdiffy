use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub relative_date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub name: String,
    pub branch: String,
    pub root: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefCapabilities {
    pub reviews: bool,
    pub revert: bool,
    pub staleness: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefDiffArgs {
    pub args: Vec<String>,
    pub include_untracked: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeEntry {
    #[serde(rename = "type")]
    pub kind: TreeEntryType,
    pub path: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TreeEntryType {
    Blob,
    Tree,
}

pub struct CommitQuery {
    pub count: u32,
    pub skip: u32,
    pub search: Option<String>,
}
