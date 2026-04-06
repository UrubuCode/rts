use std::collections::{BTreeMap, BTreeSet};

use super::nodes::{HirImport, HirItem, HirModule};

#[derive(Debug, Clone, Copy, Default)]
pub struct HirOptimizationReport {
    pub deduplicated_imports: usize,
    pub removed_noops: usize,
    pub simplified_statements: usize,
    pub inlined_calls: usize,
}

pub fn optimize(module: &mut HirModule) -> HirOptimizationReport {
    let mut report = HirOptimizationReport::default();
    let mut new_items = Vec::with_capacity(module.items.len());
    let mut seen_imports = BTreeSet::<String>::new();
    let mut constants = BTreeMap::<String, (String, String)>::new();
    let mut inline_calls = BTreeMap::<String, String>::new();

    for item in std::mem::take(&mut module.items) {
        match item {
            HirItem::Import(import) => {
                let canonical = canonicalize_import(import);
                let key = format!("{}::{}", canonical.from, canonical.names.join(","));
                if !seen_imports.insert(key) {
                    report.deduplicated_imports += 1;
                    continue;
                }
                new_items.push(HirItem::Import(canonical));
            }
            HirItem::Statement(statement) => {
                let trimmed = statement.trim();
                if trimmed.is_empty() || trimmed == "noop" {
                    report.removed_noops += usize::from(trimmed == "noop");
                    continue;
                }

                if let Some(declaration) = parse_declaration(trimmed) {
                    let mut expr = declaration.expr.clone();
                    let mut inferred_type = infer_literal_type(&expr);

                    if let Some((value, ty)) = constants.get(expr.trim()) {
                        expr = value.clone();
                        inferred_type = Some(ty.clone());
                        report.simplified_statements += 1;
                    }

                    if declaration.keyword == "const" {
                        if let Some((inline_name, inline_body)) =
                            parse_inline_arrow_assignment(&declaration)
                        {
                            inline_calls.insert(inline_name, inline_body);
                        }

                        if let Some(ty) = inferred_type.clone() {
                            constants.insert(declaration.name.clone(), (expr.clone(), ty));
                        }
                    } else {
                        constants.remove(&declaration.name);
                    }

                    let annotation = declaration.explicit_type.or(inferred_type);
                    let rebuilt = if let Some(annotation) = annotation {
                        format!(
                            "{} {}: {} = {};",
                            declaration.keyword, declaration.name, annotation, expr
                        )
                    } else {
                        format!("{} {} = {};", declaration.keyword, declaration.name, expr)
                    };

                    if rebuilt != trimmed {
                        report.simplified_statements += 1;
                    }

                    new_items.push(HirItem::Statement(rebuilt));
                    continue;
                }

                if let Some(callee) = parse_zero_arg_call(trimmed) {
                    if let Some(inline_body) = inline_calls.get(callee) {
                        report.inlined_calls += 1;
                        new_items.push(HirItem::Statement(inline_body.clone()));
                        continue;
                    }
                }

                new_items.push(HirItem::Statement(trimmed.to_string()));
            }
            HirItem::Function(function) => new_items.push(HirItem::Function(function)),
            HirItem::Interface(interface) => new_items.push(HirItem::Interface(interface)),
            HirItem::Class(class) => new_items.push(HirItem::Class(class)),
        }
    }

    module.items = new_items;
    module.imports = module
        .items
        .iter()
        .filter_map(|item| {
            if let HirItem::Import(import) = item {
                Some(import.clone())
            } else {
                None
            }
        })
        .collect();

    report
}

#[derive(Debug, Clone)]
struct Declaration {
    keyword: String,
    name: String,
    explicit_type: Option<String>,
    expr: String,
}

fn parse_declaration(line: &str) -> Option<Declaration> {
    let text = line.trim().trim_end_matches(';').trim();
    let (keyword, rest) = if let Some(rest) = text.strip_prefix("const ") {
        ("const", rest)
    } else if let Some(rest) = text.strip_prefix("let ") {
        ("let", rest)
    } else if let Some(rest) = text.strip_prefix("var ") {
        ("var", rest)
    } else {
        return None;
    };

    let (left, right) = rest.split_once('=')?;
    let left = left.trim();
    let expr = right.trim().to_string();

    let (name, explicit_type) = if let Some((name, ty)) = left.split_once(':') {
        (name.trim(), Some(ty.trim().to_string()))
    } else {
        (left, None)
    };

    if !is_identifier(name) {
        return None;
    }

    Some(Declaration {
        keyword: keyword.to_string(),
        name: name.to_string(),
        explicit_type,
        expr,
    })
}

fn parse_inline_arrow_assignment(declaration: &Declaration) -> Option<(String, String)> {
    if declaration.keyword != "const" {
        return None;
    }

    let text = declaration.expr.trim();
    let body = text.strip_prefix("() =>")?.trim();

    if body.starts_with('{') && body.ends_with('}') {
        let inner = body[1..body.len() - 1].trim().trim_end_matches(';').trim();
        if inner.ends_with(')') {
            return Some((declaration.name.clone(), format!("{};", inner)));
        }
        return None;
    }

    if body.ends_with(')') {
        return Some((
            declaration.name.clone(),
            format!("{};", body.trim_end_matches(';').trim()),
        ));
    }

    None
}

fn parse_zero_arg_call(line: &str) -> Option<&str> {
    let text = line.trim().trim_end_matches(';').trim();
    let name = text.strip_suffix("()")?.trim();
    if is_identifier(name) {
        Some(name)
    } else {
        None
    }
}

fn infer_literal_type(expr: &str) -> Option<String> {
    let text = expr.trim();

    if parse_string_literal(text).is_some() {
        return Some("string".to_string());
    }

    match text {
        "true" | "false" => return Some("boolean".to_string()),
        "null" => return Some("null".to_string()),
        "undefined" => return Some("undefined".to_string()),
        _ => {}
    }

    if let Some(ty) = infer_number_type(text) {
        return Some(ty.to_string());
    }

    if text.starts_with('[') && text.ends_with(']') {
        let inner = text[1..text.len() - 1].trim();
        if inner.is_empty() {
            return Some("any[]".to_string());
        }

        let mut elem_type = None::<String>;
        for part in split_top_level(inner, ',') {
            let inferred = infer_literal_type(part.trim()).unwrap_or_else(|| "any".to_string());
            if let Some(existing) = &elem_type {
                if existing != &inferred {
                    elem_type = Some("any".to_string());
                    break;
                }
            } else {
                elem_type = Some(inferred);
            }
        }

        return Some(format!(
            "{}[]",
            elem_type.unwrap_or_else(|| "any".to_string())
        ));
    }

    if text.starts_with('{') && text.ends_with('}') {
        let inner = text[1..text.len() - 1].trim();
        if inner.is_empty() {
            return Some("{}".to_string());
        }

        let mut parts = Vec::new();
        for chunk in split_top_level(inner, ',') {
            let (key, value) = chunk.split_once(':')?;
            let key = key.trim().trim_matches('"').trim_matches('\'');
            let value_type = infer_literal_type(value.trim()).unwrap_or_else(|| "any".to_string());
            parts.push(format!("{}: {}", key, value_type));
        }

        return Some(format!("{{ {} }}", parts.join(", ")));
    }

    if text.starts_with("Symbol(") && text.ends_with(')') {
        return Some("symbol".to_string());
    }

    if text.starts_with("BigInt(") && text.ends_with(')') {
        return Some("bigint".to_string());
    }

    None
}

fn infer_number_type(text: &str) -> Option<&'static str> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()) {
        let value = text.parse::<i64>().ok()?;
        return Some(if (i8::MIN as i64..=i8::MAX as i64).contains(&value) {
            "i8"
        } else if (i16::MIN as i64..=i16::MAX as i64).contains(&value) {
            "i16"
        } else if (i32::MIN as i64..=i32::MAX as i64).contains(&value) {
            "i32"
        } else {
            "i64"
        });
    }

    if text.contains('.') && text.parse::<f64>().is_ok() {
        return Some("f64");
    }

    None
}

fn canonicalize_import(mut import: HirImport) -> HirImport {
    import.names = import
        .names
        .into_iter()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();
    import.names.sort();
    import.names.dedup();
    import
}

fn parse_string_literal(expr: &str) -> Option<()> {
    if expr.len() < 2 {
        return None;
    }

    let first = expr.chars().next()?;
    if !matches!(first, '\'' | '"' | '`') {
        return None;
    }

    if expr.ends_with(first) {
        Some(())
    } else {
        None
    }
}

fn split_top_level(input: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut quote = '\0';
    let mut escape = false;
    let mut paren = 0i32;
    let mut bracket = 0i32;
    let mut brace = 0i32;

    for (index, ch) in input.char_indices() {
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

        if matches!(ch, '\'' | '"' | '`') {
            quote = ch;
            continue;
        }

        match ch {
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            _ => {}
        }

        if ch == separator && paren == 0 && bracket == 0 && brace == 0 {
            parts.push(input[start..index].trim());
            start = index + ch.len_utf8();
        }
    }

    parts.push(input[start..].trim());
    parts
}

fn is_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if first != '_' && first != '$' && !first.is_ascii_alphabetic() {
        return false;
    }

    chars.all(|ch| ch == '_' || ch == '$' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::{HirItem, HirModule, optimize};

    #[test]
    fn deduplicates_imports_and_simplifies_const_alias() {
        let mut module = HirModule {
            items: vec![
                HirItem::Import(super::HirImport {
                    names: vec!["print".to_string()],
                    from: "rts".to_string(),
                }),
                HirItem::Import(super::HirImport {
                    names: vec!["print".to_string()],
                    from: "rts".to_string(),
                }),
                HirItem::Statement("const valor = 1".to_string()),
                HirItem::Statement("const valor2 = valor".to_string()),
            ],
            ..Default::default()
        };

        let report = optimize(&mut module);
        assert_eq!(report.deduplicated_imports, 1);

        assert!(module.items.iter().any(
            |item| matches!(item, HirItem::Statement(text) if text == "const valor: i8 = 1;")
        ));
        assert!(module.items.iter().any(
            |item| matches!(item, HirItem::Statement(text) if text == "const valor2: i8 = 1;")
        ));
    }

    #[test]
    fn inlines_simple_zero_arg_arrow_call() {
        let mut module = HirModule {
            items: vec![
                HirItem::Statement("const valor11 = () => { console.log(\"Hello\") }".to_string()),
                HirItem::Statement("valor11()".to_string()),
            ],
            ..Default::default()
        };

        let report = optimize(&mut module);
        assert_eq!(report.inlined_calls, 1);

        assert!(module.items.iter().any(
            |item| matches!(item, HirItem::Statement(text) if text == "console.log(\"Hello\");")
        ));
    }
}
