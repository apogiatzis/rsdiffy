use std::collections::HashSet;

use crate::error::Result;
use crate::exec::{git_exec, git_exec_large, git_exec_lines, git_exec_with_stdin};
use crate::types::RefDiffArgs;

pub const WORKING_TREE_REFS: &[&str] = &["work", ".", "staged", "unstaged"];

pub fn get_diff(args: &[&str]) -> Result<String> {
    let mut cmd_args = vec!["diff"];
    cmd_args.extend_from_slice(args);
    git_exec_large(&cmd_args)
}

pub fn get_untracked_files() -> Result<Vec<String>> {
    git_exec_lines(&["ls-files", "--others", "--exclude-standard"])
}

pub fn get_untracked_diff(files: &[String]) -> String {
    let mut diffs = Vec::new();

    for file in files {
        // git diff --no-index exits with 1 when files differ, which is expected.
        // We need to capture stdout even on "failure".
        let output = std::process::Command::new("git")
            .args(["diff", "--no-index", "--", "/dev/null", file])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        if let Ok(out) = output {
            // Exit code 1 means files differ (expected for untracked files)
            if out.status.code() == Some(1) {
                let stdout = String::from_utf8_lossy(&out.stdout);
                if !stdout.is_empty() {
                    diffs.push(stdout.to_string());
                }
            }
        }
    }

    diffs.join("\n")
}

pub fn resolve_diff_args(git_ref: &str) -> RefDiffArgs {
    match git_ref {
        "staged" => RefDiffArgs {
            args: vec!["--staged".to_string()],
            include_untracked: false,
        },
        "unstaged" => RefDiffArgs {
            args: vec![],
            include_untracked: false,
        },
        "." | "work" => RefDiffArgs {
            args: vec!["HEAD".to_string()],
            include_untracked: true,
        },
        _ => RefDiffArgs {
            args: vec![normalize_ref(git_ref).unwrap_or_else(|_| git_ref.to_string())],
            include_untracked: true,
        },
    }
}

pub fn resolve_ref(git_ref: &str, extra_args: &[&str]) -> Result<String> {
    let resolved = resolve_diff_args(git_ref);

    let mut args: Vec<&str> = resolved.args.iter().map(|s| s.as_str()).collect();
    args.extend_from_slice(extra_args);
    let mut raw = get_diff(&args)?;

    if resolved.include_untracked {
        let untracked_files = get_untracked_files()?;
        if !untracked_files.is_empty() {
            let untracked_diff = get_untracked_diff(&untracked_files);
            if !untracked_diff.is_empty() {
                raw.push('\n');
                raw.push_str(&untracked_diff);
            }
        }
    }

    Ok(raw)
}

pub fn get_diff_files(git_ref: &str) -> Result<Vec<String>> {
    let resolved = resolve_diff_args(git_ref);

    let mut cmd_args: Vec<&str> = vec!["diff", "--name-only"];
    let refs: Vec<&str> = resolved.args.iter().map(|s| s.as_str()).collect();
    cmd_args.extend_from_slice(&refs);
    let tracked = git_exec_lines(&cmd_args)?;

    if resolved.include_untracked {
        let untracked = get_untracked_files()?;
        let mut all: HashSet<String> = HashSet::new();
        for f in tracked {
            all.insert(f);
        }
        for f in untracked {
            all.insert(f);
        }
        Ok(all.into_iter().collect())
    } else {
        Ok(tracked)
    }
}

pub fn get_diff_stat(args: &[&str]) -> String {
    let mut cmd_args = vec!["diff", "--stat"];
    cmd_args.extend_from_slice(args);
    git_exec_large(&cmd_args).unwrap_or_default()
}

pub fn get_diff_stat_for_ref(git_ref: &str) -> String {
    let resolved = resolve_diff_args(git_ref);
    let args: Vec<&str> = resolved.args.iter().map(|s| s.as_str()).collect();
    let mut stat = get_diff_stat(&args);

    if resolved.include_untracked {
        if let Ok(untracked) = get_untracked_files() {
            if !untracked.is_empty() {
                stat.push('\n');
                stat.push_str(&untracked.join("\n"));
            }
        }
    }

    stat
}

pub fn revert_file(file_path: &str, is_untracked: bool) -> Result<()> {
    if is_untracked {
        std::fs::remove_file(file_path).map_err(|e| crate::error::GitError::CommandFailed {
            cmd: format!("remove {}", file_path),
            stderr: e.to_string(),
        })?;
    } else {
        git_exec(&["checkout", "HEAD", "--", file_path])?;
    }
    Ok(())
}

pub fn revert_hunk(patch: &str) -> Result<()> {
    git_exec_with_stdin(&["apply", "--reverse", "--unidiff-zero"], patch)?;
    Ok(())
}

pub fn get_merge_base(a: &str, b: &str) -> Result<String> {
    git_exec(&["merge-base", a, b])
}

pub fn normalize_ref(git_ref: &str) -> Result<String> {
    if git_ref.contains("...") {
        return Ok(git_ref.to_string());
    }

    if let Some(idx) = git_ref.find("..") {
        // Make sure it's ".." not "..."
        let after = &git_ref[idx + 2..];
        if !after.starts_with('.') {
            let left = &git_ref[..idx];
            let right = after;
            let base = get_merge_base(left, right)?;
            return Ok(format!("{}..{}", base, right));
        }
    }

    get_merge_base(git_ref, "HEAD")
}

pub fn resolve_base_ref(git_ref: &str) -> Result<String> {
    if WORKING_TREE_REFS.contains(&git_ref) {
        return Ok("HEAD".to_string());
    }

    if let Some(idx) = git_ref.find("...") {
        let left = &git_ref[..idx];
        let right = &git_ref[idx + 3..];
        return get_merge_base(left, right);
    }

    if let Some(idx) = git_ref.find("..") {
        let after = &git_ref[idx + 2..];
        if !after.starts_with('.') {
            let left = &git_ref[..idx];
            let right = after;
            return get_merge_base(left, right);
        }
    }

    get_merge_base(git_ref, "HEAD")
}

pub fn get_file_content(path: &str, git_ref: &str) -> Result<String> {
    git_exec(&["show", &format!("{}:{}", git_ref, path)])
}

pub fn get_file_line_count(path: &str, git_ref: &str) -> Option<u32> {
    let content = git_exec(&["show", &format!("{}:{}", git_ref, path)]).ok()?;
    Some(content.split('\n').count() as u32)
}
