use crate::error::Result;
use crate::exec::git_exec;
use crate::types::{Commit, CommitQuery};

pub fn get_recent_commits(query: &CommitQuery) -> Result<Vec<Commit>> {
    let count_arg = format!("-n{}", query.count);
    let skip_arg = format!("--skip={}", query.skip);
    let format_arg = "--format=%H|%h|%s|%cr".to_string();

    let mut args: Vec<String> = vec!["log".to_string(), count_arg, skip_arg, format_arg];

    if let Some(ref search) = query.search {
        args.push(format!("--grep={}", search));
        args.push("-i".to_string());
    }

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let output = git_exec(&args_refs)?;

    if output.is_empty() {
        return Ok(Vec::new());
    }

    Ok(output
        .split('\n')
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() == 4 {
                Some(Commit {
                    hash: parts[0].to_string(),
                    short_hash: parts[1].to_string(),
                    message: parts[2].to_string(),
                    relative_date: parts[3].to_string(),
                })
            } else {
                None
            }
        })
        .collect())
}
