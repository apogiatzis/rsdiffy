use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineDiffType {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordDiffSegment {
    pub text: String,
    #[serde(rename = "type")]
    pub kind: LineDiffType,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffLineType {
    Add,
    Delete,
    Context,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    #[serde(rename = "type")]
    pub kind: DiffLineType,
    pub content: String,
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_newline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_diff: Option<Vec<WordDiffSegment>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub header: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub old_path: String,
    pub new_path: String,
    pub status: FileStatus,
    pub hunks: Vec<DiffHunk>,
    pub additions: u32,
    pub deletions: u32,
    pub is_binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_file_line_count: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffStats {
    pub total_additions: u32,
    pub total_deletions: u32,
    pub files_changed: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedDiff {
    pub files: Vec<DiffFile>,
    pub stats: DiffStats,
}
