use rsdiffy_parser::{compute_word_diff, LineDiffType, WordDiffSegment};

fn seg(text: &str, kind: LineDiffType) -> WordDiffSegment {
    WordDiffSegment {
        text: text.to_string(),
        kind,
    }
}

fn eq(text: &str) -> WordDiffSegment {
    seg(text, LineDiffType::Equal)
}

fn del(text: &str) -> WordDiffSegment {
    seg(text, LineDiffType::Delete)
}

fn ins(text: &str) -> WordDiffSegment {
    seg(text, LineDiffType::Insert)
}

#[test]
fn returns_equal_segment_for_identical_lines() {
    let result = compute_word_diff("hello world", "hello world");
    assert_eq!(result, vec![eq("hello world")]);
}

#[test]
fn detects_single_word_change() {
    let result = compute_word_diff("const x = 5;", "const x = 10;");
    assert_eq!(
        result,
        vec![eq("const x = "), del("5"), ins("10"), eq(";")]
    );
}

#[test]
fn detects_word_added_at_end() {
    let result = compute_word_diff("hello", "hello world");
    assert_eq!(result, vec![eq("hello"), ins(" world")]);
}

#[test]
fn detects_word_deleted_from_beginning() {
    let result = compute_word_diff("const x = 1;", "x = 1;");
    assert_eq!(result, vec![del("const "), eq("x = 1;")]);
}

#[test]
fn detects_multiple_word_changes() {
    let result = compute_word_diff(
        r#"import { foo } from "bar";"#,
        r#"import { baz } from "qux";"#,
    );
    assert_eq!(
        result,
        vec![
            eq("import { "),
            del("foo"),
            ins("baz"),
            eq(r#" } from ""#),
            del("bar"),
            ins("qux"),
            eq(r#"";"#),
        ]
    );
}

#[test]
fn handles_entirely_different_lines() {
    let result = compute_word_diff("abc", "xyz");
    assert_eq!(result, vec![del("abc"), ins("xyz")]);
}

#[test]
fn handles_empty_old_line() {
    let result = compute_word_diff("", "hello");
    assert_eq!(result, vec![ins("hello")]);
}

#[test]
fn handles_empty_new_line() {
    let result = compute_word_diff("hello", "");
    assert_eq!(result, vec![del("hello")]);
}

#[test]
fn handles_indentation_change() {
    let result = compute_word_diff("  return x;", "    return x;");
    assert_eq!(
        result,
        vec![del("  "), ins("    "), eq("return x;")]
    );
}

#[test]
fn handles_case_change() {
    let result = compute_word_diff("Foo", "foo");
    assert_eq!(result, vec![del("Foo"), ins("foo")]);
}

#[test]
fn handles_string_content_change() {
    let result = compute_word_diff(
        r#"const msg = "hello";"#,
        r#"const msg = "world";"#,
    );
    assert_eq!(
        result,
        vec![
            eq(r#"const msg = ""#),
            del("hello"),
            ins("world"),
            eq(r#"";"#),
        ]
    );
}

#[test]
fn handles_whitespace_only_change_between_words() {
    let result = compute_word_diff("a  b", "a b");
    assert_eq!(
        result,
        vec![eq("a"), del("  "), ins(" "), eq("b")]
    );
}
