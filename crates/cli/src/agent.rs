use std::sync::OnceLock;

use regex::Regex;

use crate::db;
use crate::threads;

static MENTION_RE: OnceLock<Regex> = OnceLock::new();

fn mention_regex() -> &'static Regex {
    MENTION_RE.get_or_init(|| Regex::new(r"(?is)@(claude|codex)\b\s*(.*)").unwrap())
}

/// Detect an @agent mention in comment text.
/// Returns `(agent_name, instruction)` if found.
pub fn detect_agent_mention(text: &str) -> Option<(String, String)> {
    let re = mention_regex();
    re.captures(text).map(|caps| {
        let agent = caps[1].to_lowercase();
        let instruction = caps[2].trim().to_string();
        (agent, instruction)
    })
}

/// Map agent name to CLI command + args with pre-granted permissions.
/// Returns `None` for unsupported agents.
pub fn resolve_agent_command(agent: &str) -> Option<(String, Vec<String>)> {
    match agent {
        "claude" => Some((
            "claude".to_string(),
            vec![
                "-p".to_string(),
                "--allowedTools".to_string(),
                "Read,Grep,Glob,Bash(git diff *),Bash(git show *),Bash(git log *)".to_string(),
                "--max-turns".to_string(),
                "10".to_string(),
            ],
        )),
        "codex" => Some((
            "codex".to_string(),
            vec!["exec".to_string(), "--full-auto".to_string()],
        )),
        _ => None,
    }
}

/// Invoke an agent synchronously. Pipes `prompt` to stdin, returns stdout.
/// This is a blocking call — use inside `spawn_blocking` when called from async context.
pub fn invoke_agent(agent: &str, prompt: &str) -> Result<String, String> {
    let (cmd, args) =
        resolve_agent_command(agent).ok_or_else(|| format!("Unsupported agent '{}'", agent))?;

    let output = std::process::Command::new(&cmd)
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(prompt.as_bytes());
            }
            child.wait_with_output()
        });

    match output {
        Ok(o) if o.status.success() => Ok(String::from_utf8_lossy(&o.stdout).to_string()),
        Ok(o) => Err(format!("Agent '{}' exited with status {}", agent, o.status)),
        Err(e) => Err(format!(
            "Failed to run '{}': {}. Is it installed and in PATH?",
            cmd, e
        )),
    }
}

/// Build a conversational prompt for a discussion thread @mention.
#[allow(clippy::too_many_arguments)]
pub fn build_discussion_prompt(
    file_path: &str,
    side: &str,
    start_line: i64,
    end_line: i64,
    anchor_content: Option<&str>,
    file_diff: &str,
    conversation: &[(String, String)],
    instruction: &str,
) -> String {
    let instruction = if instruction.is_empty() {
        "Please review this code and share your thoughts."
    } else {
        instruction
    };

    let anchor_section = if let Some(content) = anchor_content {
        format!(
            "\nCode snippet under discussion:\n```\n{}\n```\n",
            content
        )
    } else {
        String::new()
    };

    let diff_section = if file_diff.trim().is_empty() {
        String::from("(No diff available for this file)")
    } else {
        format!("```diff\n{}\n```", file_diff.trim())
    };

    let conversation_section = if conversation.is_empty() {
        String::from("(No prior messages)")
    } else {
        conversation
            .iter()
            .map(|(author, body)| format!("**{}:** {}", author, body))
            .collect::<Vec<_>>()
            .join("\n\n")
    };

    format!(
        r#"You are an AI assistant participating in a code review discussion.

## Discussion Context
File: {file_path} (lines {start_line}-{end_line}, {side} side)
{anchor_section}
## Diff for this file
{diff_section}

Focus your review on the diff above — these are the changes under review. The comment is anchored at lines {start_line}-{end_line}.
If you need additional context beyond the diff, read the file `{file_path}` or related files.

## Conversation So Far
{conversation_section}

## Your Task
{instruction}

Respond directly in markdown. Be concise, specific, and helpful.
Do not wrap your response in JSON or code fences (unless showing code examples)."#
    )
}

/// Spawn an agent reply in the background. Invokes the agent and inserts
/// the response as a reply in the given thread.
pub async fn spawn_agent_reply(thread_id: String, agent: String, prompt: String) {
    let agent_name = agent.clone();
    let result = tokio::task::spawn_blocking(move || invoke_agent(&agent_name, &prompt)).await;

    let reply_body = match result {
        Ok(Ok(text)) => {
            let trimmed = text.trim().to_string();
            if trimmed.is_empty() {
                return;
            }
            trimmed
        }
        Ok(Err(e)) => format!("*Agent error: {}*", e),
        Err(e) => format!("*Agent task failed: {}*", e),
    };

    if let Ok(conn) = db::get_db() {
        let _ = threads::add_reply(&conn, &thread_id, &reply_body, &agent, "bot");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_claude_mention() {
        let result = detect_agent_mention("@claude review this code");
        assert_eq!(result, Some(("claude".to_string(), "review this code".to_string())));
    }

    #[test]
    fn detect_codex_mention() {
        let result = detect_agent_mention("@codex explain this function");
        assert_eq!(result, Some(("codex".to_string(), "explain this function".to_string())));
    }

    #[test]
    fn detect_case_insensitive() {
        let result = detect_agent_mention("@CLAUDE review");
        assert_eq!(result, Some(("claude".to_string(), "review".to_string())));
    }

    #[test]
    fn detect_mid_text() {
        let result = detect_agent_mention("Hey @claude can you help?");
        assert_eq!(result, Some(("claude".to_string(), "can you help?".to_string())));
    }

    #[test]
    fn detect_bare_mention() {
        let result = detect_agent_mention("@claude");
        assert_eq!(result, Some(("claude".to_string(), String::new())));
    }

    #[test]
    fn no_match_unsupported() {
        assert_eq!(detect_agent_mention("@bob review this"), None);
    }

    #[test]
    fn no_match_email() {
        // Word boundary prevents matching inside emails
        assert_eq!(detect_agent_mention("email user@claudeapp.com"), None);
    }

    #[test]
    fn resolve_claude() {
        let result = resolve_agent_command("claude");
        assert!(result.is_some());
        let (cmd, args) = result.unwrap();
        assert_eq!(cmd, "claude");
        assert!(args.contains(&"-p".to_string()));
        assert!(args.contains(&"--allowedTools".to_string()));
    }

    #[test]
    fn resolve_codex() {
        let result = resolve_agent_command("codex");
        assert!(result.is_some());
        let (cmd, args) = result.unwrap();
        assert_eq!(cmd, "codex");
        assert!(args.contains(&"--full-auto".to_string()));
    }

    #[test]
    fn resolve_unsupported() {
        assert!(resolve_agent_command("gpt").is_none());
    }

    #[test]
    fn detect_multiline_instruction() {
        let result = detect_agent_mention("@claude review this\nand also check for bugs");
        assert!(result.is_some());
        let (agent, instruction) = result.unwrap();
        assert_eq!(agent, "claude");
        assert!(instruction.contains("review this"));
        assert!(instruction.contains("check for bugs"));
    }

    #[test]
    fn detect_no_mention() {
        assert_eq!(detect_agent_mention("just a regular comment"), None);
    }

    #[test]
    fn prompt_includes_diff() {
        let prompt = build_discussion_prompt(
            "src/main.rs",
            "new",
            10,
            20,
            Some("fn main() {}"),
            "+fn main() { println!(\"hello\"); }",
            &[],
            "review this",
        );
        assert!(prompt.contains("src/main.rs"));
        assert!(prompt.contains("lines 10-20"));
        assert!(prompt.contains("fn main() {}"));
        assert!(prompt.contains("+fn main() { println!(\"hello\"); }"));
        assert!(prompt.contains("review this"));
    }

    #[test]
    fn prompt_default_instruction_when_empty() {
        let prompt = build_discussion_prompt(
            "lib.rs", "new", 1, 5, None, "some diff", &[], "",
        );
        assert!(prompt.contains("review this code and share your thoughts"));
    }

    #[test]
    fn prompt_includes_conversation() {
        let conversation = vec![
            ("alice".to_string(), "I think this has a bug".to_string()),
            ("bob".to_string(), "Which line?".to_string()),
        ];
        let prompt = build_discussion_prompt(
            "lib.rs", "new", 1, 5, None, "diff", &conversation, "help",
        );
        assert!(prompt.contains("**alice:** I think this has a bug"));
        assert!(prompt.contains("**bob:** Which line?"));
    }

    #[test]
    fn prompt_empty_diff_shows_fallback() {
        let prompt = build_discussion_prompt(
            "lib.rs", "new", 1, 5, None, "  ", &[], "review",
        );
        assert!(prompt.contains("No diff available"));
    }

    #[test]
    fn prompt_no_anchor_content() {
        let prompt = build_discussion_prompt(
            "lib.rs", "new", 1, 5, None, "diff here", &[], "review",
        );
        // Should not contain "Code snippet under discussion" when no anchor
        assert!(!prompt.contains("Code snippet under discussion"));
    }

    #[test]
    fn prompt_with_anchor_content() {
        let prompt = build_discussion_prompt(
            "lib.rs", "new", 1, 5, Some("let x = 42;"), "diff", &[], "review",
        );
        assert!(prompt.contains("Code snippet under discussion"));
        assert!(prompt.contains("let x = 42;"));
    }
}
