use rsdiffy_parser::{parse_diff, DiffLineType, LineDiffType};

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    );
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn attaches_word_diff_to_paired_delete_add_lines() {
    let result = parse_diff(&fixture("single-file-deletions.diff"));
    let hunk = &result.files[0].hunks[0];

    let del_lines: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Delete)
        .collect();
    let add_lines: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Add)
        .collect();

    assert!(del_lines[0].word_diff.is_some());
    assert!(add_lines[0].word_diff.is_some());
}

#[test]
fn does_not_attach_word_diff_to_pure_additions() {
    let result = parse_diff(&fixture("single-file-additions.diff"));
    let hunk = &result.files[0].hunks[0];
    let add_lines: Vec<_> = hunk
        .lines
        .iter()
        .filter(|l| l.kind == DiffLineType::Add)
        .collect();

    for line in add_lines {
        assert!(line.word_diff.is_none());
    }
}

#[test]
fn pairs_consecutive_delete_add_sequences_for_word_diff() {
    let result = parse_diff(&fixture("single-file-mixed.diff"));
    let hunk = &result.files[0].hunks[0];

    let port_del = hunk
        .lines
        .iter()
        .find(|l| l.kind == DiffLineType::Delete && l.content.contains("3000"));
    let port_add = hunk
        .lines
        .iter()
        .find(|l| l.kind == DiffLineType::Add && l.content.contains("8080"));

    assert!(port_del.unwrap().word_diff.is_some());
    assert!(port_add.unwrap().word_diff.is_some());

    let segments = port_del.unwrap().word_diff.as_ref().unwrap();
    let has_delete = segments.iter().any(|s| s.kind == LineDiffType::Delete);
    assert!(has_delete);
}
