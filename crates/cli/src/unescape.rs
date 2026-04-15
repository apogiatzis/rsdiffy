/// Remove shell-introduced backslash escapes from markdown text.
/// AI agents constructing shell commands often produce escaped strings.
pub fn unescape_markdown(text: &str) -> String {
    text.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\`", "`")
        .replace("\\\"", "\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_newlines() {
        assert_eq!(unescape_markdown("line1\\nline2"), "line1\nline2");
    }

    #[test]
    fn unescape_tabs() {
        assert_eq!(unescape_markdown("col1\\tcol2"), "col1\tcol2");
    }

    #[test]
    fn unescape_backticks() {
        assert_eq!(unescape_markdown("use \\`code\\`"), "use `code`");
    }

    #[test]
    fn unescape_quotes() {
        assert_eq!(unescape_markdown("say \\\"hello\\\""), "say \"hello\"");
    }

    #[test]
    fn unescape_mixed() {
        assert_eq!(
            unescape_markdown("line1\\nuse \\`fn()\\`\\n\\\"done\\\""),
            "line1\nuse `fn()`\n\"done\""
        );
    }

    #[test]
    fn unescape_no_escapes() {
        assert_eq!(unescape_markdown("plain text"), "plain text");
    }

    #[test]
    fn unescape_empty() {
        assert_eq!(unescape_markdown(""), "");
    }
}
