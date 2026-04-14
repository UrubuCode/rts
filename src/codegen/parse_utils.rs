pub(crate) fn split_top_level(input: &str, separator: char) -> Vec<&str> {
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
