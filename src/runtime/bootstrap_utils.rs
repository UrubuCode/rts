pub fn split_top_level(input: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut quote = '\0';
    let mut escape = false;
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;

    for (idx, ch) in input.char_indices() {
        if quote != '\0' {
            if escape {
                escape = false;
                continue;
            }

            if ch == '\\' {
                escape = true;
                continue;
            }

            if ch == quote {
                quote = '\0';
            }
            continue;
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            quote = ch;
            continue;
        }

        match ch {
            '(' => {
                paren_depth += 1;
                continue;
            }
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                continue;
            }
            '[' => {
                bracket_depth += 1;
                continue;
            }
            ']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
                continue;
            }
            '{' => {
                brace_depth += 1;
                continue;
            }
            '}' => {
                if brace_depth > 0 {
                    brace_depth -= 1;
                }
                continue;
            }
            _ => {}
        }

        if ch == separator && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            parts.push(input[start..idx].trim());
            start = idx + ch.len_utf8();
        }
    }

    parts.push(input[start..].trim());
    parts
}

pub fn split_top_level_once(input: &str, separator: char) -> Option<(&str, &str)> {
    let mut quote = '\0';
    let mut escape = false;
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut brace_depth = 0i32;

    for (idx, ch) in input.char_indices() {
        if quote != '\0' {
            if escape {
                escape = false;
                continue;
            }

            if ch == '\\' {
                escape = true;
                continue;
            }

            if ch == quote {
                quote = '\0';
            }
            continue;
        }

        if ch == '\'' || ch == '"' || ch == '`' {
            quote = ch;
            continue;
        }

        match ch {
            '(' => {
                paren_depth += 1;
                continue;
            }
            ')' => {
                if paren_depth > 0 {
                    paren_depth -= 1;
                }
                continue;
            }
            '[' => {
                bracket_depth += 1;
                continue;
            }
            ']' => {
                if bracket_depth > 0 {
                    bracket_depth -= 1;
                }
                continue;
            }
            '{' => {
                brace_depth += 1;
                continue;
            }
            '}' => {
                if brace_depth > 0 {
                    brace_depth -= 1;
                }
                continue;
            }
            _ => {}
        }

        if ch == separator && paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 {
            let left = input[..idx].trim();
            let right = input[idx + ch.len_utf8()..].trim();
            return Some((left, right));
        }
    }

    None
}

pub fn strip_comment(line: &str) -> &str {
    if let Some(index) = line.find("//") {
        &line[..index]
    } else {
        line
    }
}

pub fn brace_delta(line: &str) -> i32 {
    let open = line.chars().filter(|ch| *ch == '{').count() as i32;
    let close = line.chars().filter(|ch| *ch == '}').count() as i32;
    open - close
}

pub fn normalize(line: &str) -> &str {
    line.trim()
}

pub fn is_identifier_like(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !is_identifier_start(first) {
        return false;
    }

    chars.all(is_identifier_continue)
}

pub fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

pub fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}
