use rsdiffy_parser::{parse_diff, DiffLineType, FileStatus};

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(path).unwrap()
}

// ── basic formats ──────────────────────────────────────────────────

#[test]
fn parses_single_file_with_additions_only() {
    let result = parse_diff(&fixture("single-file-additions.diff"));

    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].old_path, "hello.ts");
    assert_eq!(result.files[0].new_path, "hello.ts");
    assert_eq!(result.files[0].status, FileStatus::Modified);
    assert_eq!(result.files[0].hunks.len(), 1);

    let hunk = &result.files[0].hunks[0];
    assert_eq!(hunk.old_start, 1);
    assert_eq!(hunk.old_count, 3);
    assert_eq!(hunk.new_start, 1);
    assert_eq!(hunk.new_count, 6);

    let add_lines: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Add)
        .collect();
    assert_eq!(add_lines.len(), 3);
    assert_eq!(add_lines[0].content, "const name = 'world';");
    assert_eq!(add_lines[0].new_line_number, Some(2));
    assert_eq!(add_lines[0].old_line_number, None);

    let context_lines: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Context)
        .collect();
    assert_eq!(context_lines.len(), 3);
    assert_eq!(context_lines[0].old_line_number, Some(1));
    assert_eq!(context_lines[0].new_line_number, Some(1));

    assert_eq!(result.files[0].additions, 3);
    assert_eq!(result.files[0].deletions, 0);
}

#[test]
fn parses_single_file_with_deletions() {
    let result = parse_diff(&fixture("single-file-deletions.diff"));

    assert_eq!(result.files.len(), 1);
    let hunk = &result.files[0].hunks[0];
    let del_lines: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Delete)
        .collect();
    assert_eq!(del_lines.len(), 3);
    assert_eq!(del_lines[0].content, "  console.log('adding', a, b);");
    assert_eq!(del_lines[0].old_line_number, Some(2));
    assert_eq!(del_lines[0].new_line_number, None);

    assert_eq!(result.files[0].additions, 1);
    assert_eq!(result.files[0].deletions, 3);
}

#[test]
fn parses_single_file_with_mixed_changes() {
    let result = parse_diff(&fixture("single-file-mixed.diff"));

    assert_eq!(result.files.len(), 1);
    let hunk = &result.files[0].hunks[0];

    let adds = hunk.lines.iter().filter(|l| l.kind == DiffLineType::Add).count();
    let dels = hunk.lines.iter().filter(|l| l.kind == DiffLineType::Delete).count();
    let ctx = hunk.lines.iter().filter(|l| l.kind == DiffLineType::Context).count();

    assert_eq!(adds, 3);
    assert_eq!(dels, 2);
    assert_eq!(ctx, 5);

    assert_eq!(result.files[0].additions, 3);
    assert_eq!(result.files[0].deletions, 2);
}

#[test]
fn parses_single_file_with_multiple_hunks() {
    let result = parse_diff(&fixture("single-file-multi-hunk.diff"));

    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].hunks.len(), 2);

    assert_eq!(result.files[0].hunks[0].old_start, 1);
    assert_eq!(result.files[0].hunks[0].new_start, 1);
    assert_eq!(result.files[0].hunks[1].old_start, 20);
    assert_eq!(result.files[0].hunks[1].new_start, 20);
}

#[test]
fn parses_multiple_files() {
    let result = parse_diff(&fixture("multi-file.diff"));

    assert_eq!(result.files.len(), 2);
    assert_eq!(result.files[0].new_path, "index.ts");
    assert_eq!(result.files[1].new_path, "runner.ts");

    assert_eq!(result.stats.files_changed, 2);
    assert_eq!(
        result.stats.total_additions,
        result.files[0].additions + result.files[1].additions
    );
    assert_eq!(
        result.stats.total_deletions,
        result.files[0].deletions + result.files[1].deletions
    );
}

#[test]
fn parses_empty_diff() {
    let result = parse_diff(&fixture("empty.diff"));

    assert_eq!(result.files.len(), 0);
    assert_eq!(result.stats.files_changed, 0);
    assert_eq!(result.stats.total_additions, 0);
    assert_eq!(result.stats.total_deletions, 0);
}

// ── hunk header parsing ────────────────────────────────────────────

#[test]
fn parses_standard_hunk_header() {
    let result = parse_diff(&fixture("single-file-additions.diff"));
    let hunk = &result.files[0].hunks[0];

    assert_eq!(hunk.old_start, 1);
    assert_eq!(hunk.old_count, 3);
    assert_eq!(hunk.new_start, 1);
    assert_eq!(hunk.new_count, 6);
}

#[test]
fn parses_hunk_header_with_function_context() {
    let result = parse_diff(&fixture("hunk-with-context.diff"));
    let hunk = &result.files[0].hunks[0];

    assert_eq!(hunk.old_start, 10);
    assert_eq!(hunk.old_count, 5);
    assert_eq!(hunk.new_start, 10);
    assert_eq!(hunk.new_count, 7);
    assert_eq!(hunk.context.as_deref(), Some("function processData() {"));
}

#[test]
fn parses_new_file_hunk_header_with_0_0() {
    let result = parse_diff(&fixture("new-file.diff"));
    let hunk = &result.files[0].hunks[0];

    assert_eq!(hunk.old_start, 0);
    assert_eq!(hunk.old_count, 0);
    assert_eq!(hunk.new_start, 1);
    assert_eq!(hunk.new_count, 3);
}

// ── line number assignment ─────────────────────────────────────────

#[test]
fn assigns_correct_line_numbers_to_context_lines() {
    let result = parse_diff(&fixture("single-file-additions.diff"));
    let context_lines: Vec<_> = result.files[0].hunks[0]
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Context)
        .collect();

    assert_eq!(context_lines[0].old_line_number, Some(1));
    assert_eq!(context_lines[0].new_line_number, Some(1));
}

#[test]
fn assigns_correct_line_numbers_to_addition_lines() {
    let result = parse_diff(&fixture("single-file-additions.diff"));
    let add_lines: Vec<_> = result.files[0].hunks[0]
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Add)
        .collect();

    assert_eq!(add_lines[0].old_line_number, None);
    assert_eq!(add_lines[0].new_line_number, Some(2));
    assert_eq!(add_lines[1].new_line_number, Some(3));
    assert_eq!(add_lines[2].new_line_number, Some(4));
}

#[test]
fn assigns_correct_line_numbers_to_deletion_lines() {
    let result = parse_diff(&fixture("single-file-deletions.diff"));
    let del_lines: Vec<_> = result.files[0].hunks[0]
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Delete)
        .collect();

    assert_eq!(del_lines[0].new_line_number, None);
    assert_eq!(del_lines[0].old_line_number, Some(2));
    assert_eq!(del_lines[1].old_line_number, Some(3));
    assert_eq!(del_lines[2].old_line_number, Some(4));
}

// ── file status detection ──────────────────────────────────────────

#[test]
fn detects_new_files() {
    let result = parse_diff(&fixture("new-file.diff"));

    assert_eq!(result.files[0].status, FileStatus::Added);
    assert_eq!(result.files[0].old_path, "/dev/null");
    assert_eq!(result.files[0].new_path, "newfile.ts");
}

#[test]
fn detects_deleted_files() {
    let result = parse_diff(&fixture("deleted-file.diff"));

    assert_eq!(result.files[0].status, FileStatus::Deleted);
    assert_eq!(result.files[0].old_path, "old-file.ts");
    assert_eq!(result.files[0].new_path, "/dev/null");
}

#[test]
fn detects_renamed_files() {
    let result = parse_diff(&fixture("renamed-file.diff"));

    assert_eq!(result.files[0].status, FileStatus::Renamed);
    assert_eq!(result.files[0].old_path, "old-name.ts");
    assert_eq!(result.files[0].new_path, "new-name.ts");
    assert_eq!(result.files[0].similarity_index, Some(85));
}

#[test]
fn detects_copied_files() {
    let result = parse_diff(&fixture("copied-file.diff"));

    assert_eq!(result.files[0].status, FileStatus::Copied);
    assert_eq!(result.files[0].old_path, "original.ts");
    assert_eq!(result.files[0].new_path, "copy.ts");
    assert_eq!(result.files[0].similarity_index, Some(90));
}

#[test]
fn detects_mode_only_changes() {
    let result = parse_diff(&fixture("mode-change.diff"));

    assert_eq!(result.files[0].status, FileStatus::Modified);
    assert_eq!(result.files[0].old_mode.as_deref(), Some("100644"));
    assert_eq!(result.files[0].new_mode.as_deref(), Some("100755"));
    assert_eq!(result.files[0].hunks.len(), 0);
}

#[test]
fn detects_mode_change_with_content() {
    let result = parse_diff(&fixture("mode-change-with-content.diff"));

    assert_eq!(result.files[0].old_mode.as_deref(), Some("100644"));
    assert_eq!(result.files[0].new_mode.as_deref(), Some("100755"));
    assert_eq!(result.files[0].hunks.len(), 1);
    assert_eq!(result.files[0].additions, 1);
}

// ── edge cases ─────────────────────────────────────────────────────

#[test]
fn handles_no_newline_at_end_of_file() {
    let result = parse_diff(&fixture("no-newline.diff"));
    let lines = &result.files[0].hunks[0].lines;
    let last_context = lines
        .iter()
        .rev()
        .find(|l| l.kind == DiffLineType::Context);

    assert_eq!(last_context.unwrap().no_newline, Some(true));
}

#[test]
fn handles_binary_files_new() {
    let result = parse_diff(&fixture("binary-file.diff"));

    assert!(result.files[0].is_binary);
    assert_eq!(result.files[0].status, FileStatus::Added);
    assert_eq!(result.files[0].hunks.len(), 0);
    assert_eq!(result.files[0].additions, 0);
    assert_eq!(result.files[0].deletions, 0);
}

#[test]
fn handles_binary_files_modified() {
    let result = parse_diff(&fixture("binary-modified.diff"));

    assert!(result.files[0].is_binary);
    assert_eq!(result.files[0].status, FileStatus::Modified);
}

#[test]
fn handles_binary_files_deleted() {
    let result = parse_diff(&fixture("binary-deleted.diff"));

    assert!(result.files[0].is_binary);
    assert_eq!(result.files[0].status, FileStatus::Deleted);
}

#[test]
fn handles_files_with_spaces_in_path() {
    let result = parse_diff(&fixture("spaces-in-path.diff"));

    assert_eq!(result.files[0].old_path, "my folder/my file.ts");
    assert_eq!(result.files[0].new_path, "my folder/my file.ts");
}

#[test]
fn handles_unicode_content() {
    let result = parse_diff(&fixture("unicode-content.diff"));

    assert_eq!(result.files.len(), 1);
    let add_line = result.files[0].hunks[0]
        .lines
        .iter()
        .find(|l| l.kind == DiffLineType::Add && l.content.contains("emoji"));
    assert!(add_line.is_some());
}

#[test]
fn handles_submodule_changes() {
    let result = parse_diff(&fixture("submodule.diff"));

    assert_eq!(result.files.len(), 1);
    assert_eq!(result.files[0].new_path, "vendor/lib");
}

// ── stats computation ──────────────────────────────────────────────

#[test]
fn computes_per_file_stats() {
    let result = parse_diff(&fixture("single-file-mixed.diff"));

    assert_eq!(result.files[0].additions, 3);
    assert_eq!(result.files[0].deletions, 2);
}

#[test]
fn computes_total_stats_across_files() {
    let result = parse_diff(&fixture("multi-file.diff"));

    assert_eq!(result.stats.files_changed, 2);
    assert!(result.stats.total_additions > 0);
    assert!(result.stats.total_deletions > 0);
}

#[test]
fn binary_files_report_0_additions_deletions() {
    let result = parse_diff(&fixture("binary-file.diff"));

    assert_eq!(result.files[0].additions, 0);
    assert_eq!(result.files[0].deletions, 0);
}
