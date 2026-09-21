//! JSONC to JSON: what a hand-written config file is, made into what a parser
//! accepts.
//!
//! # Why it lives here and not beside its first caller
//!
//! It was written in `rts-cli`, for `package.json`. `rts-cli` DEPENDS on this
//! crate, so the loader could not call it, and the loader needs it for
//! `tsconfig.json`. The choice was to copy it down or move it down; a copy is
//! two answers to "what is a comment", and the two would drift the first time
//! one of them learned a form the other did not — which is exactly what this
//! version does to the original by learning block comments.
//!
//! # What it does NOT do
//!
//! It is not a JSON parser and does not validate. It removes comments and
//! trailing commas so that a real parser can read what is left. A malformed
//! file is the parser's error to report, with the parser's message.

/// JSONC to JSON: comments and trailing commas removed, everything else kept.
///
/// Byte offsets are NOT preserved — a comment becomes nothing, not spaces —
/// so an error position from the parser indexes the stripped text. No caller
/// reports positions today; one that wants to must keep the original instead
/// of asking for it back from here.
pub fn strip(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            output.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                output.push(ch);
            }
            '/' if matches!(chars.peek(), Some('/')) => {
                let _ = chars.next();
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            }
            // A block comment keeps the newlines it spanned, so a line number
            // computed on the stripped text still means the same line.
            '/' if matches!(chars.peek(), Some('*')) => {
                let _ = chars.next();
                let mut previous = '\0';
                for next in chars.by_ref() {
                    if next == '\n' {
                        output.push('\n');
                    }
                    if previous == '*' && next == '/' {
                        break;
                    }
                    previous = next;
                }
            }
            ',' => {
                // A comma is trailing when the next thing that is not space or
                // a comment closes the container. Decided by looking, because
                // the alternative is a second pass that has to agree with this
                // one about what a comment is.
                let mut lookahead = chars.clone();
                let mut next_real = None;
                while let Some(peeked) = lookahead.next() {
                    match peeked {
                        c if c.is_whitespace() => continue,
                        '/' if matches!(lookahead.peek(), Some('/')) => {
                            for skipped in lookahead.by_ref() {
                                if skipped == '\n' {
                                    break;
                                }
                            }
                        }
                        '/' if matches!(lookahead.peek(), Some('*')) => {
                            let _ = lookahead.next();
                            let mut previous = '\0';
                            for skipped in lookahead.by_ref() {
                                if previous == '*' && skipped == '/' {
                                    break;
                                }
                                previous = skipped;
                            }
                        }
                        other => {
                            next_real = Some(other);
                            break;
                        }
                    }
                }
                if !matches!(next_real, Some('}') | Some(']')) {
                    output.push(ch);
                }
            }
            other => output.push(other),
        }
    }

    output
}
