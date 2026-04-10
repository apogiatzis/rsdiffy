use crate::error::Result;
use crate::exec::{git_exec, git_exec_lines};

pub fn get_staged_files() -> Result<Vec<String>> {
    git_exec_lines(&["diff", "--staged", "--name-only"])
}

pub fn get_unstaged_files() -> Result<Vec<String>> {
    git_exec_lines(&["diff", "--name-only"])
}

pub fn is_dirty() -> Result<bool> {
    let output = git_exec(&["status", "--porcelain"])?;
    Ok(!output.is_empty())
}
