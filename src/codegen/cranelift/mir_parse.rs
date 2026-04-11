use super::parse_utils::split_top_level;

pub(crate) const ABI_ARG_SLOTS: usize = 6;
pub(crate) const ABI_PARAM_COUNT: usize = ABI_ARG_SLOTS + 1; // argc + args
pub(crate) const ABI_UNDEFINED_HANDLE: i64 = 0;
/// Dispatch unificado: __rts_dispatch(fn_id, a0..a5) -> i64
pub(crate) const RTS_DISPATCH_SYMBOL: &str = "__rts_dispatch";
/// Dispatch dinâmico por string para callees não resolvidos: __rts_call_dispatch(ptr, len, argc, a0..a5) -> i64
pub(crate) const RTS_CALL_DISPATCH_SYMBOL: &str = "__rts_call_dispatch";

#[derive(Debug, Clone)]
pub(crate) struct ParsedCall {
    pub callee: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ParsedDeclaration {
    pub name: String,
    pub initializer: Option<String>,
    pub mutable: bool,
}

pub(crate) fn parse_return_literal(statement: &str) -> Option<i64> {
    let literal = statement.strip_prefix("ret ")?;
    literal.trim().parse::<i64>().ok()
}

pub(crate) fn parse_return_expression(statement: &str) -> Option<Option<String>> {
    let text = statement.trim().trim_end_matches(';').trim();
    let rest = text.strip_prefix("return")?;
    if !rest.is_empty() {
        let first = rest.chars().next()?;
        if first == '_' || first == '$' || first.is_ascii_alphanumeric() {
            return None;
        }
    }
    let expression = rest.trim();
    if expression.is_empty() {
        Some(None)
    } else {
        Some(Some(expression.to_string()))
    }
}

pub(crate) fn parse_enter_parameters(statement: &str) -> Option<Vec<String>> {
    let text = statement.trim();
    let rest = text.strip_prefix("enter ")?;
    let open = rest.find('(')?;
    if !rest.ends_with(')') {
        return None;
    }

    let args = &rest[open + 1..rest.len().saturating_sub(1)];
    if args.trim().is_empty() {
        return Some(Vec::new());
    }

    let names = split_top_level(args, ',')
        .into_iter()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToString::to_string)
        .collect::<Vec<_>>();

    Some(names)
}

pub(crate) fn parse_call_statement(statement: &str) -> Option<ParsedCall> {
    let text = statement.trim().trim_end_matches(';').trim();
    if text.is_empty() {
        return None;
    }

    if let Some(rest) = text.strip_prefix("call ") {
        let rest = rest.trim();
        if rest.is_empty() {
            return None;
        }

        if let Some(parsed) = parse_call_invocation(rest) {
            return Some(parsed);
        }

        return is_valid_callee_name(rest).then(|| ParsedCall {
            callee: rest.to_string(),
            args: Vec::new(),
        });
    }

    parse_call_invocation(text)
}

pub(crate) fn parse_declaration_statement(statement: &str) -> Option<ParsedDeclaration> {
    let text = statement.trim().trim_end_matches(';').trim();
    let (rest, mutable) = if let Some(rest) = text.strip_prefix("const ") {
        (rest, false)
    } else if let Some(rest) = text.strip_prefix("let ") {
        (rest, true)
    } else if let Some(rest) = text.strip_prefix("var ") {
        (rest, true)
    } else {
        return None;
    };

    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }

    let (left, initializer) = if let Some((left, right)) = rest.split_once('=') {
        (left.trim(), {
            let value = right.trim().to_string();
            (!value.is_empty()).then_some(value)
        })
    } else {
        (rest, None)
    };

    let name = if let Some((name, _type_annotation)) = left.split_once(':') {
        name.trim()
    } else {
        left
    };

    if !is_valid_binding_name(name) {
        return None;
    }

    Some(ParsedDeclaration {
        name: name.to_string(),
        initializer,
        mutable,
    })
}

pub(crate) fn parse_call_invocation(text: &str) -> Option<ParsedCall> {
    let open = text.find('(')?;
    if !text.ends_with(')') {
        return None;
    }

    let callee = text[..open].trim();
    if !is_valid_callee_name(callee) {
        return None;
    }

    let args_raw = text[open + 1..text.len().saturating_sub(1)].trim();
    let args = if args_raw.is_empty() {
        Vec::new()
    } else {
        split_top_level(args_raw, ',')
            .into_iter()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };

    Some(ParsedCall {
        callee: callee.to_string(),
        args,
    })
}

pub(crate) fn is_valid_callee_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '$' | '.' | ':'))
}

pub(crate) fn is_valid_binding_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if first != '_' && first != '$' && !first.is_ascii_alphabetic() {
        return false;
    }

    chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}
