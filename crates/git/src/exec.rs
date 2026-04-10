use std::process::{Command, Stdio};

use crate::error::{GitError, Result};

/// Run a git subcommand with direct argument passing (no shell).
/// Returns trimmed stdout. Fails on non-zero exit.
pub fn git_exec(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        return Err(GitError::CommandFailed {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a git subcommand, return full stdout (not trimmed). For large outputs like diffs.
pub fn git_exec_large(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        return Err(GitError::CommandFailed {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run a git subcommand with stdin piped in. Returns trimmed stdout.
pub fn git_exec_with_stdin(args: &[&str], input: &str) -> Result<String> {
    use std::io::Write;

    let mut child = Command::new("git")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let output = child.wait_with_output()?;

    if !output.status.success() {
        return Err(GitError::CommandFailed {
            cmd: format!("git {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Run a git subcommand, split trimmed output into lines. Empty output returns empty vec.
pub fn git_exec_lines(args: &[&str]) -> Result<Vec<String>> {
    let output = git_exec(args)?;
    if output.is_empty() {
        return Ok(Vec::new());
    }
    Ok(output.split('\n').map(|s| s.to_string()).collect())
}
