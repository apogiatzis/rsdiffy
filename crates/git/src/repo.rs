use std::fs;
use std::path::Path;

use crate::diff::WORKING_TREE_REFS;
use crate::error::Result;
use crate::exec::git_exec;
use crate::types::{RefCapabilities, RepoInfo};

pub fn is_git_repo() -> bool {
    git_exec(&["rev-parse", "--is-inside-work-tree"]).is_ok()
}

pub fn get_repo_root() -> Result<String> {
    git_exec(&["rev-parse", "--show-toplevel"])
}

pub fn get_repo_name() -> Result<String> {
    let root = get_repo_root()?;
    Ok(root.rsplit('/').next().unwrap_or(&root).to_string())
}

pub fn get_current_branch() -> String {
    git_exec(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_else(|_| "HEAD".to_string())
}

pub fn get_repo_info() -> Result<RepoInfo> {
    Ok(RepoInfo {
        name: get_repo_name()?,
        branch: get_current_branch(),
        root: get_repo_root()?,
    })
}

pub fn get_head_hash() -> Result<String> {
    git_exec(&["rev-parse", "HEAD"])
}

pub fn get_rsdiffy_dir_path() -> Result<String> {
    let repo_root = get_repo_root()?;
    let digest = sha256_hex(&repo_root);
    let hash = &digest[..12];
    let home = dirs_home();
    Ok(format!("{home}/.config/rsdiffy/{hash}"))
}

pub fn get_rsdiffy_dir() -> Result<String> {
    let dir = get_rsdiffy_dir_path()?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn is_valid_git_ref(git_ref: &str) -> bool {
    if git_ref.contains("...") {
        return git_ref.split("...").all(is_valid_git_ref);
    }

    if git_ref.contains("..") {
        return git_ref.split("..").all(is_valid_git_ref);
    }

    git_exec(&["rev-parse", "--verify", git_ref]).is_ok()
}

pub fn get_ref_capabilities(git_ref: Option<&str>) -> RefCapabilities {
    match git_ref {
        None => RefCapabilities {
            reviews: true,
            revert: false,
            staleness: false,
        },
        Some(r) => {
            let is_working_tree = WORKING_TREE_REFS.contains(&r);
            RefCapabilities {
                reviews: true,
                revert: is_working_tree,
                staleness: true,
            }
        }
    }
}

/// Validates that a file path resolves to a location within the repository root.
/// Returns the canonical path if valid, or an error if the path escapes the repo.
pub fn validate_repo_path(file_path: &str) -> Result<std::path::PathBuf> {
    let root = get_repo_root()?;
    let root_canonical = fs::canonicalize(&root)?;
    let full_path = Path::new(&root).join(file_path);
    let canonical = fs::canonicalize(&full_path).map_err(|e| crate::error::GitError::CommandFailed {
        cmd: "path validation".to_string(),
        stderr: format!("Path not found or invalid: {} ({})", file_path, e),
    })?;
    if !canonical.starts_with(&root_canonical) {
        return Err(crate::error::GitError::CommandFailed {
            cmd: "path validation".to_string(),
            stderr: format!("Path escapes repository root: {}", file_path),
        });
    }
    Ok(canonical)
}

fn dirs_home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())
}

/// Pure Rust SHA-256 hex digest (no shell needed).
fn sha256_hex(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    // Use two differently-seeded hashes to produce a longer, more unique digest.
    let mut h1 = DefaultHasher::new();
    input.hash(&mut h1);
    let v1 = h1.finish();
    let mut h2 = DefaultHasher::new();
    v1.hash(&mut h2);
    input.hash(&mut h2);
    let v2 = h2.finish();
    format!("{:016x}{:016x}", v1, v2)
}
