use regex::Regex;
use std::sync::LazyLock;

use crate::types::*;
use crate::word_diff::compute_word_diff;

static DIFF_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^diff --git a/(.*) b/(.*)$").unwrap());
static HUNK_HEADER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@(.*)$").unwrap());
static OLD_FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^--- (.+)$").unwrap());
static NEW_FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\+\+\+ (.+)$").unwrap());
static SIMILARITY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^similarity index (\d+)%$").unwrap());
static RENAME_FROM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^rename from (.+)$").unwrap());
static RENAME_TO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^rename to (.+)$").unwrap());
static COPY_FROM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^copy from (.+)$").unwrap());
static COPY_TO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^copy to (.+)$").unwrap());
static OLD_MODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^old mode (\d+)$").unwrap());
static NEW_MODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^new mode (\d+)$").unwrap());
static NEW_FILE_MODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^new file mode (\d+)$").unwrap());
static DELETED_FILE_MODE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^deleted file mode (\d+)$").unwrap());
static BINARY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^Binary files (.+) and (.+) differ$").unwrap());
static NO_NEWLINE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\\ No newline at end of file$").unwrap());

fn strip_prefix(path: &str) -> &str {
    if path == "/dev/null" {
        return path;
    }
    if let Some(stripped) = path.strip_prefix("a/").or_else(|| path.strip_prefix("b/")) {
        return stripped;
    }
    path
}

fn attach_word_diffs(hunk: &mut DiffHunk) {
    let lines = &mut hunk.lines;
    let mut i = 0;

    while i < lines.len() {
        if lines[i].kind == DiffLineType::Delete {
            let delete_start = i;
            while i < lines.len() && lines[i].kind == DiffLineType::Delete {
                i += 1;
            }
            let delete_end = i;

            let add_start = i;
            while i < lines.len() && lines[i].kind == DiffLineType::Add {
                i += 1;
            }
            let add_end = i;

            let delete_count = delete_end - delete_start;
            let add_count = add_end - add_start;

            if delete_count > 0 && add_count > 0 {
                let pair_count = delete_count.min(add_count);
                for p in 0..pair_count {
                    let del_content = lines[delete_start + p].content.clone();
                    let add_content = lines[add_start + p].content.clone();
                    let segments = compute_word_diff(&del_content, &add_content);
                    lines[delete_start + p].word_diff = Some(segments.clone());
                    lines[add_start + p].word_diff = Some(segments);
                }
            }
        } else {
            i += 1;
        }
    }
}

pub fn parse_diff(raw: &str) -> ParsedDiff {
    let lines: Vec<&str> = raw.split('\n').collect();
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file: Option<DiffFile> = None;
    let mut current_hunk_idx: Option<usize> = None;
    let mut old_line_num: u32 = 0;
    let mut new_line_num: u32 = 0;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // diff header
        if let Some(caps) = DIFF_HEADER_RE.captures(line) {
            if let Some(file) = current_file.take() {
                files.push(file);
            }
            current_file = Some(DiffFile {
                old_path: caps[1].to_string(),
                new_path: caps[2].to_string(),
                status: FileStatus::Modified,
                hunks: Vec::new(),
                additions: 0,
                deletions: 0,
                is_binary: false,
                old_mode: None,
                new_mode: None,
                similarity_index: None,
                old_file_line_count: None,
            });
            current_hunk_idx = None;
            i += 1;
            continue;
        }

        let file = match current_file.as_mut() {
            Some(f) => f,
            None => {
                i += 1;
                continue;
            }
        };

        // old mode
        if let Some(caps) = OLD_MODE_RE.captures(line) {
            file.old_mode = Some(caps[1].to_string());
            i += 1;
            continue;
        }

        // new mode
        if let Some(caps) = NEW_MODE_RE.captures(line) {
            file.new_mode = Some(caps[1].to_string());
            i += 1;
            continue;
        }

        // new file mode
        if let Some(caps) = NEW_FILE_MODE_RE.captures(line) {
            file.new_mode = Some(caps[1].to_string());
            file.status = FileStatus::Added;
            i += 1;
            continue;
        }

        // deleted file mode
        if let Some(caps) = DELETED_FILE_MODE_RE.captures(line) {
            file.old_mode = Some(caps[1].to_string());
            file.status = FileStatus::Deleted;
            i += 1;
            continue;
        }

        // similarity index
        if let Some(caps) = SIMILARITY_RE.captures(line) {
            file.similarity_index = Some(caps[1].parse().unwrap());
            i += 1;
            continue;
        }

        // rename from
        if let Some(caps) = RENAME_FROM_RE.captures(line) {
            file.old_path = caps[1].to_string();
            file.status = FileStatus::Renamed;
            i += 1;
            continue;
        }

        // rename to
        if let Some(caps) = RENAME_TO_RE.captures(line) {
            file.new_path = caps[1].to_string();
            file.status = FileStatus::Renamed;
            i += 1;
            continue;
        }

        // copy from
        if let Some(caps) = COPY_FROM_RE.captures(line) {
            file.old_path = caps[1].to_string();
            file.status = FileStatus::Copied;
            i += 1;
            continue;
        }

        // copy to
        if let Some(caps) = COPY_TO_RE.captures(line) {
            file.new_path = caps[1].to_string();
            file.status = FileStatus::Copied;
            i += 1;
            continue;
        }

        // old file path (--- a/...)
        if let Some(caps) = OLD_FILE_RE.captures(line) {
            let path = strip_prefix(&caps[1]);
            file.old_path = path.to_string();
            if path == "/dev/null" {
                file.status = FileStatus::Added;
            }
            i += 1;
            continue;
        }

        // new file path (+++ b/...)
        if let Some(caps) = NEW_FILE_RE.captures(line) {
            let path = strip_prefix(&caps[1]);
            file.new_path = path.to_string();
            if path == "/dev/null" {
                file.status = FileStatus::Deleted;
            }
            i += 1;
            continue;
        }

        // binary files
        if BINARY_RE.is_match(line) {
            file.is_binary = true;
            i += 1;
            continue;
        }

        // hunk header
        if let Some(caps) = HUNK_HEADER_RE.captures(line) {
            let old_start: u32 = caps[1].parse().unwrap();
            let old_count: u32 = caps.get(2).map_or(1, |m| m.as_str().parse().unwrap());
            let new_start: u32 = caps[3].parse().unwrap();
            let new_count: u32 = caps.get(4).map_or(1, |m| m.as_str().parse().unwrap());
            let context_str = caps.get(5).map(|m| m.as_str().trim()).unwrap_or("");
            let context = if context_str.is_empty() {
                None
            } else {
                Some(context_str.to_string())
            };

            let hunk = DiffHunk {
                header: line.to_string(),
                old_start,
                old_count,
                new_start,
                new_count,
                context,
                lines: Vec::new(),
            };
            file.hunks.push(hunk);
            current_hunk_idx = Some(file.hunks.len() - 1);
            old_line_num = old_start;
            new_line_num = new_start;
            i += 1;
            continue;
        }

        // no newline at end of file
        if NO_NEWLINE_RE.is_match(line) {
            if let Some(hunk_idx) = current_hunk_idx {
                let hunk = &mut file.hunks[hunk_idx];
                if let Some(last_line) = hunk.lines.last_mut() {
                    last_line.no_newline = Some(true);
                }
            }
            i += 1;
            continue;
        }

        // diff content lines
        if let Some(hunk_idx) = current_hunk_idx {
            let prefix = line.as_bytes().first().copied();
            let content = if line.len() > 1 { &line[1..] } else { "" };

            match prefix {
                Some(b'+') => {
                    file.hunks[hunk_idx].lines.push(DiffLine {
                        kind: DiffLineType::Add,
                        content: content.to_string(),
                        old_line_number: None,
                        new_line_number: Some(new_line_num),
                        no_newline: None,
                        word_diff: None,
                    });
                    file.additions += 1;
                    new_line_num += 1;
                }
                Some(b'-') => {
                    file.hunks[hunk_idx].lines.push(DiffLine {
                        kind: DiffLineType::Delete,
                        content: content.to_string(),
                        old_line_number: Some(old_line_num),
                        new_line_number: None,
                        no_newline: None,
                        word_diff: None,
                    });
                    file.deletions += 1;
                    old_line_num += 1;
                }
                Some(b' ') => {
                    file.hunks[hunk_idx].lines.push(DiffLine {
                        kind: DiffLineType::Context,
                        content: content.to_string(),
                        old_line_number: Some(old_line_num),
                        new_line_number: Some(new_line_num),
                        no_newline: None,
                        word_diff: None,
                    });
                    old_line_num += 1;
                    new_line_num += 1;
                }
                _ => {}
            }
        }

        i += 1;
    }

    if let Some(file) = current_file.take() {
        files.push(file);
    }

    // Attach word diffs
    for file in &mut files {
        for hunk in &mut file.hunks {
            attach_word_diffs(hunk);
        }
    }

    // Compute stats
    let mut total_additions: u32 = 0;
    let mut total_deletions: u32 = 0;
    for file in &files {
        total_additions += file.additions;
        total_deletions += file.deletions;
    }
    let files_changed = files.len() as u32;

    ParsedDiff {
        files,
        stats: DiffStats {
            total_additions,
            total_deletions,
            files_changed,
        },
    }
}
