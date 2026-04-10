/// Remove shell-introduced backslash escapes from markdown text.
/// AI agents constructing shell commands often produce escaped strings.
pub fn unescape_markdown(text: &str) -> String {
    text.replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\`", "`")
        .replace("\\\"", "\"")
}
