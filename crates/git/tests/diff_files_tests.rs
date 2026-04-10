use std::fs;
use std::process::Command;
use tempfile::TempDir;

/// Helper to run a git command in a specific directory.
fn git(repo_dir: &str, args: &str) {
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!("cd '{}' && git {}", repo_dir, args))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("failed to run git");
    assert!(status.success(), "git {} failed in {}", args, repo_dir);
}

/// Helper to write a file in the repo.
fn write_file(repo_dir: &str, name: &str, content: &str) {
    let path = format!("{}/{}", repo_dir, name);
    if let Some(parent) = std::path::Path::new(&path).parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, content).unwrap();
}

/// Set up a test repo with:
/// - main branch with base.txt (initial commit)
/// - feature branch with feature.txt + modified base.txt
/// - main branch with master-only.txt (diverged)
/// - cwd set to feature branch
struct TestRepo {
    _dir: TempDir,
    path: String,
    orig_dir: String,
}

impl TestRepo {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let orig_dir = std::env::current_dir()
            .unwrap()
            .to_string_lossy()
            .to_string();

        git(&path, "init -b main");
        git(&path, "config user.email \"test@test.com\"");
        git(&path, "config user.name \"Test\"");

        write_file(&path, "base.txt", "base content\n");
        git(&path, "add .");
        git(&path, "commit -m \"initial commit\"");

        git(&path, "checkout -b feature");
        write_file(&path, "feature.txt", "feature content\n");
        write_file(&path, "base.txt", "modified base\n");
        git(&path, "add .");
        git(&path, "commit -m \"feature changes\"");

        git(&path, "checkout main");
        write_file(&path, "master-only.txt", "master content\n");
        git(&path, "add .");
        git(&path, "commit -m \"master changes\"");

        git(&path, "checkout feature");

        std::env::set_current_dir(&path).unwrap();

        TestRepo {
            _dir: dir,
            path,
            orig_dir,
        }
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.orig_dir);
    }
}

#[test]
fn returns_files_changed_on_branch_vs_base_ref() {
    let repo = TestRepo::new();
    let files = rsdiffy_git::diff::get_diff_files("main").unwrap();

    assert!(files.contains(&"feature.txt".to_string()));
    assert!(files.contains(&"base.txt".to_string()));
    assert!(!files.contains(&"master-only.txt".to_string()));

    drop(repo);
}

#[test]
fn returns_only_branch_changes_even_when_base_has_diverged() {
    let repo = TestRepo::new();
    let files = rsdiffy_git::diff::get_diff_files("main").unwrap();

    assert_eq!(files.len(), 2);
    assert!(!files.contains(&"master-only.txt".to_string()));

    drop(repo);
}

#[test]
fn returns_staged_files_for_staged_ref() {
    let repo = TestRepo::new();

    write_file(&repo.path, "staged-file.txt", "staged\n");
    git(&repo.path, "add staged-file.txt");

    let files = rsdiffy_git::diff::get_diff_files("staged").unwrap();
    assert!(files.contains(&"staged-file.txt".to_string()));

    // Cleanup
    git(&repo.path, "reset HEAD staged-file.txt");
    let _ = fs::remove_file(format!("{}/staged-file.txt", repo.path));

    drop(repo);
}

#[test]
fn returns_unstaged_files_for_unstaged_ref() {
    let repo = TestRepo::new();

    write_file(&repo.path, "base.txt", "unstaged change\n");

    let files = rsdiffy_git::diff::get_diff_files("unstaged").unwrap();
    assert!(files.contains(&"base.txt".to_string()));

    // Cleanup
    git(&repo.path, "checkout -- base.txt");

    drop(repo);
}

#[test]
fn returns_working_tree_files_for_work_ref() {
    let repo = TestRepo::new();

    write_file(&repo.path, "untracked-file.txt", "untracked\n");
    write_file(&repo.path, "base.txt", "modified\n");
    git(&repo.path, "add base.txt");

    let files = rsdiffy_git::diff::get_diff_files("work").unwrap();
    assert!(files.contains(&"base.txt".to_string()));
    assert!(files.contains(&"untracked-file.txt".to_string()));

    // Cleanup
    git(&repo.path, "reset HEAD base.txt");
    git(&repo.path, "checkout -- base.txt");
    let _ = fs::remove_file(format!("{}/untracked-file.txt", repo.path));

    drop(repo);
}
