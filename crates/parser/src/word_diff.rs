use crate::types::{LineDiffType, WordDiffSegment};

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn tokenize(text: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut token_start = 0;
    let mut prev_is_word = None;

    while let Some(&(idx, ch)) = chars.peek() {
        let current_is_word = is_word_char(ch);

        match prev_is_word {
            None => {
                token_start = idx;
                prev_is_word = Some(current_is_word);
            }
            Some(prev) if current_is_word != prev => {
                tokens.push(&text[token_start..idx]);
                token_start = idx;
                prev_is_word = Some(current_is_word);
            }
            _ => {}
        }

        chars.next();
    }

    if token_start < text.len() {
        tokens.push(&text[token_start..]);
    }

    tokens
}

fn lcs<'a>(a: &[&'a str], b: &[&str]) -> Vec<&'a str> {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0u32; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push(a[i - 1]);
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] > dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    result.reverse();
    result
}

pub fn compute_word_diff(old_line: &str, new_line: &str) -> Vec<WordDiffSegment> {
    if old_line == new_line {
        return vec![WordDiffSegment {
            text: old_line.to_string(),
            kind: LineDiffType::Equal,
        }];
    }

    if old_line.is_empty() {
        return vec![WordDiffSegment {
            text: new_line.to_string(),
            kind: LineDiffType::Insert,
        }];
    }

    if new_line.is_empty() {
        return vec![WordDiffSegment {
            text: old_line.to_string(),
            kind: LineDiffType::Delete,
        }];
    }

    let old_tokens = tokenize(old_line);
    let new_tokens = tokenize(new_line);
    let common = lcs(&old_tokens, &new_tokens);

    let mut segments: Vec<WordDiffSegment> = Vec::new();
    let mut oi = 0;
    let mut ni = 0;
    let mut ci = 0;

    while ci < common.len() {
        let common_token = common[ci];

        let mut delete_text = String::new();
        while oi < old_tokens.len() && old_tokens[oi] != common_token {
            delete_text.push_str(old_tokens[oi]);
            oi += 1;
        }

        let mut insert_text = String::new();
        while ni < new_tokens.len() && new_tokens[ni] != common_token {
            insert_text.push_str(new_tokens[ni]);
            ni += 1;
        }

        if !delete_text.is_empty() {
            segments.push(WordDiffSegment {
                text: delete_text,
                kind: LineDiffType::Delete,
            });
        }
        if !insert_text.is_empty() {
            segments.push(WordDiffSegment {
                text: insert_text,
                kind: LineDiffType::Insert,
            });
        }

        let mut equal_text = String::new();
        while oi < old_tokens.len()
            && ni < new_tokens.len()
            && ci < common.len()
            && old_tokens[oi] == common[ci]
            && new_tokens[ni] == common[ci]
        {
            equal_text.push_str(old_tokens[oi]);
            oi += 1;
            ni += 1;
            ci += 1;
        }

        if !equal_text.is_empty() {
            segments.push(WordDiffSegment {
                text: equal_text,
                kind: LineDiffType::Equal,
            });
        }
    }

    let mut trailing_delete = String::new();
    while oi < old_tokens.len() {
        trailing_delete.push_str(old_tokens[oi]);
        oi += 1;
    }

    let mut trailing_insert = String::new();
    while ni < new_tokens.len() {
        trailing_insert.push_str(new_tokens[ni]);
        ni += 1;
    }

    if !trailing_delete.is_empty() {
        segments.push(WordDiffSegment {
            text: trailing_delete,
            kind: LineDiffType::Delete,
        });
    }
    if !trailing_insert.is_empty() {
        segments.push(WordDiffSegment {
            text: trailing_insert,
            kind: LineDiffType::Insert,
        });
    }

    segments
}
