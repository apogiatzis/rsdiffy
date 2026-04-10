use std::collections::{BTreeMap, HashSet};
use std::fs;

use crate::error::Result;
use crate::exec::git_exec;
use crate::types::{TreeEntry, TreeEntryType};

fn get_working_tree_files(dir_path: Option<&str>) -> Result<Vec<String>> {
    let path_suffix = dir_path.map(|d| format!("{}/", d));

    let mut tracked_args: Vec<String> = vec!["ls-files".into()];
    if let Some(ref p) = path_suffix {
        tracked_args.push(p.clone());
    }
    let tracked_refs: Vec<&str> = tracked_args.iter().map(|s| s.as_str()).collect();
    let tracked = git_exec(&tracked_refs).unwrap_or_default();

    let mut deleted_args: Vec<String> = vec!["ls-files".into(), "--deleted".into()];
    if let Some(ref p) = path_suffix {
        deleted_args.push(p.clone());
    }
    let deleted_refs: Vec<&str> = deleted_args.iter().map(|s| s.as_str()).collect();
    let deleted = git_exec(&deleted_refs).unwrap_or_default();

    let mut untracked_args: Vec<String> =
        vec!["ls-files".into(), "--others".into(), "--exclude-standard".into()];
    if let Some(ref p) = path_suffix {
        untracked_args.push(p.clone());
    }
    let untracked_refs: Vec<&str> = untracked_args.iter().map(|s| s.as_str()).collect();
    let untracked = git_exec(&untracked_refs).unwrap_or_default();

    let deleted_set: HashSet<&str> = if deleted.is_empty() {
        HashSet::new()
    } else {
        deleted.split('\n').collect()
    };

    let mut files = BTreeMap::new();

    if !tracked.is_empty() {
        for f in tracked.split('\n') {
            if !deleted_set.contains(f) {
                files.insert(f.to_string(), ());
            }
        }
    }

    if !untracked.is_empty() {
        for f in untracked.split('\n') {
            files.insert(f.to_string(), ());
        }
    }

    Ok(files.into_keys().collect())
}

pub fn get_tree() -> Result<Vec<String>> {
    get_working_tree_files(None)
}

pub fn get_tree_entries(_ref: &str, dir_path: Option<&str>) -> Result<Vec<TreeEntry>> {
    let files = get_working_tree_files(dir_path)?;
    let prefix = dir_path.map(|d| format!("{}/", d)).unwrap_or_default();
    let mut entries: BTreeMap<String, TreeEntry> = BTreeMap::new();

    for file in &files {
        let relative = if !prefix.is_empty() && file.starts_with(&prefix) {
            &file[prefix.len()..]
        } else if prefix.is_empty() {
            file.as_str()
        } else {
            continue;
        };

        if let Some(slash_idx) = relative.find('/') {
            let dir_name = &relative[..slash_idx];
            if !entries.contains_key(dir_name) {
                entries.insert(
                    dir_name.to_string(),
                    TreeEntry {
                        kind: TreeEntryType::Tree,
                        path: format!("{}{}", prefix, dir_name),
                        name: dir_name.to_string(),
                    },
                );
            }
        } else {
            entries.insert(
                relative.to_string(),
                TreeEntry {
                    kind: TreeEntryType::Blob,
                    path: file.to_string(),
                    name: relative.to_string(),
                },
            );
        }
    }

    Ok(entries.into_values().collect())
}

pub fn get_tree_fingerprint() -> Result<String> {
    let tracked = git_exec(&["ls-files"]).unwrap_or_default();
    let stat_output = git_exec(&["status", "--porcelain", "-u"]).unwrap_or_default();
    Ok(format!("{}:{}", tracked.len(), stat_output))
}

pub fn get_working_tree_file_content(file_path: &str) -> Result<String> {
    let canonical = crate::repo::validate_repo_path(file_path)?;
    Ok(fs::read_to_string(canonical)?)
}

pub fn get_working_tree_raw_file(file_path: &str) -> Result<(Vec<u8>, String)> {
    let canonical = crate::repo::validate_repo_path(file_path)?;
    let full_path_str = canonical.to_string_lossy().to_string();
    let data = fs::read(&canonical)?;
    Ok((data, full_path_str))
}
