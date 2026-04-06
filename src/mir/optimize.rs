use std::collections::{BTreeMap, BTreeSet};

use super::MirModule;

#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizationReport {
    pub inlined_calls: usize,
    pub removed_noops: usize,
    pub deduplicated_imports: usize,
    pub simplified_declarations: usize,
    pub inferred_declarations: usize,
    pub type_mismatches: usize,
}

pub fn optimize(module: &mut MirModule) -> OptimizationReport {
    let mut report = OptimizationReport::default();
    let mut seen_imports = BTreeSet::<String>::new();

    for function in &mut module.functions {
        let mut constants = BTreeMap::<String, LiteralValue>::new();
        let mut inline_calls = BTreeMap::<String, String>::new();

        for block in &mut function.blocks {
            let mut optimized = Vec::with_capacity(block.statements.len());

            for statement in std::mem::take(&mut block.statements) {
                let text = statement.text.trim();
                if text.is_empty() || text == "noop" {
                    report.removed_noops += usize::from(text == "noop");
                    continue;
                }

                if let Some(import) = parse_import_signature(text) {
                    if !seen_imports.insert(import.key) {
                        report.deduplicated_imports += 1;
                        continue;
                    }

                    optimized.push(super::MirStatement {
                        text: import.normalized,
                    });
                    continue;
                }

                if let Some(declaration) = parse_declaration(text) {
                    let declaration_text = optimize_declaration(
                        declaration,
                        &mut constants,
                        &mut inline_calls,
                        &mut report,
                    );

                    optimized.push(super::MirStatement {
                        text: declaration_text,
                    });
                    continue;
                }

                if let Some(callee) = parse_zero_arg_call(text) {
                    if let Some(inline_body) = inline_calls.get(callee) {
                        report.inlined_calls += 1;
                        optimized.push(super::MirStatement {
                            text: inline_body.clone(),
                        });
                        continue;
                    }
                }

                optimized.push(statement);
            }

            block.statements = optimized;
        }
    }

    report
}

fn optimize_declaration(
    declaration: Declaration,
    constants: &mut BTreeMap<String, LiteralValue>,
    inline_calls: &mut BTreeMap<String, String>,
    report: &mut OptimizationReport,
) -> String {
    let inference = infer_expression(&declaration.expr, constants);

    let mut final_type = declaration.explicit_type.clone();
    if final_type.is_none() {
        if let Some(inferred) = &inference.inferred_type {
            final_type = Some(inferred.clone());
            report.inferred_declarations += 1;
        }
    } else if let (Some(declared), Some(inferred)) =
        (&declaration.explicit_type, &inference.inferred_type)
    {
        if !is_type_compatible(declared, inferred) {
            report.type_mismatches += 1;
        }
    }

    if declaration.expr.trim() != inference.normalized_expr.trim() {
        report.simplified_declarations += 1;
    }

    if declaration.keyword == "const" {
        if let Some(literal) = inference.literal.clone() {
            constants.insert(declaration.name.clone(), literal);
        }

        if let Some(inline) = inference.inline_body {
            inline_calls.insert(declaration.name.clone(), inline);
        }
    } else {
        constants.remove(&declaration.name);
    }

    let mut rebuilt = String::new();
    if declaration.exported {
        rebuilt.push_str("export ");
    }

    rebuilt.push_str(&declaration.keyword);
    rebuilt.push(' ');
    rebuilt.push_str(&declaration.name);

    if let Some(annotation) = final_type {
        rebuilt.push_str(": ");
        rebuilt.push_str(&annotation);
    }

    rebuilt.push_str(" = ");
    rebuilt.push_str(&inference.normalized_expr);
    rebuilt.push(';');
    rebuilt
}

#[derive(Debug, Clone)]
struct ImportSignature {
    key: String,
    normalized: String,
}

fn parse_import_signature(line: &str) -> Option<ImportSignature> {
    let trimmed = line.trim().trim_end_matches(';').trim();
    let body = trimmed.strip_prefix("import ")?;
    let (left, right) = body.rsplit_once(" from ")?;
    let module = right
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string();

    if module.is_empty() {
        return None;
    }

    let names = normalize_import_names(left)?;
    let key = format!("{}::{}", module, names.join(","));
    let normalized = format!("import {{{}}} from \"{}\";", names.join(", "), module);

    Some(ImportSignature { key, normalized })
}

fn normalize_import_names(raw: &str) -> Option<Vec<String>> {
    let mut text = raw.trim();

    if let Some(open) = text.find('{') {
        let close = text[open + 1..].find('}')? + open + 1;
        text = &text[open + 1..close];
    }

    let mut names = text
        .split(',')
        .filter_map(|chunk| {
            let mut name = chunk.trim();
            if name.is_empty() {
                return None;
            }

            if let Some(stripped) = name.strip_prefix("type ") {
                name = stripped.trim_start();
            }

            if let Some((left, _)) = name.split_once(" as ") {
                name = left.trim();
            }

            if is_identifier(name) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if names.is_empty() {
        return None;
    }

    names.sort();
    names.dedup();
    Some(names)
}

#[derive(Debug, Clone)]
struct Declaration {
    exported: bool,
    keyword: String,
    name: String,
    explicit_type: Option<String>,
    expr: String,
}

fn parse_declaration(line: &str) -> Option<Declaration> {
    let mut text = line.trim().trim_end_matches(';').trim();
    let mut exported = false;

    if let Some(rest) = text.strip_prefix("export ") {
        exported = true;
        text = rest.trim_start();
    }

    let keyword = if let Some(rest) = text.strip_prefix("const ") {
        text = rest;
        "const"
    } else if let Some(rest) = text.strip_prefix("let ") {
        text = rest;
        "let"
    } else if let Some(rest) = text.strip_prefix("var ") {
        text = rest;
        "var"
    } else {
        return None;
    };

    let (left, right) = text.split_once('=')?;
    let left = left.trim();
    let expr = right.trim().to_string();

    let (name, explicit_type) = if let Some((binding, annotation)) = left.split_once(':') {
        (binding.trim(), Some(annotation.trim().to_string()))
    } else {
        (left, None)
    };

    if !is_identifier(name) {
        return None;
    }

    Some(Declaration {
        exported,
        keyword: keyword.to_string(),
        name: name.to_string(),
        explicit_type,
        expr,
    })
}

#[derive(Debug, Clone)]
struct InferredExpression {
    literal: Option<LiteralValue>,
    inferred_type: Option<String>,
    normalized_expr: String,
    inline_body: Option<String>,
}

fn infer_expression(expr: &str, constants: &BTreeMap<String, LiteralValue>) -> InferredExpression {
    let trimmed = expr.trim();

    if let Some(value) = constants.get(trimmed).cloned() {
        return InferredExpression {
            inferred_type: Some(value.inferred_type()),
            normalized_expr: value.render(),
            literal: Some(value),
            inline_body: None,
        };
    }

    if let Some(value) = parse_literal_value(trimmed, constants) {
        return InferredExpression {
            inferred_type: Some(value.inferred_type()),
            normalized_expr: value.render(),
            literal: Some(value),
            inline_body: None,
        };
    }

    if let Some((inferred_type, inline_body)) = parse_inline_arrow_function(trimmed) {
        return InferredExpression {
            inferred_type: Some(inferred_type),
            normalized_expr: trimmed.to_string(),
            literal: None,
            inline_body,
        };
    }

    InferredExpression {
        literal: None,
        inferred_type: None,
        normalized_expr: trimmed.to_string(),
        inline_body: None,
    }
}

fn parse_inline_arrow_function(expr: &str) -> Option<(String, Option<String>)> {
    let body = expr.strip_prefix("() =>")?.trim();

    if body.starts_with('{') && body.ends_with('}') {
        let inner = body[1..body.len() - 1].trim();
        if inner.is_empty() {
            return Some(("() => void".to_string(), None));
        }

        let statement = inner.trim_end_matches(';').trim();
        if statement.ends_with(')') {
            return Some(("() => void".to_string(), Some(format!("{};", statement))));
        }

        return Some(("() => unknown".to_string(), None));
    }

    if body.ends_with(')') {
        return Some((
            "() => void".to_string(),
            Some(format!("{};", body.trim_end_matches(';').trim())),
        ));
    }

    Some(("() => unknown".to_string(), None))
}

fn parse_literal_value(
    expr: &str,
    constants: &BTreeMap<String, LiteralValue>,
) -> Option<LiteralValue> {
    if let Some(text) = parse_string_literal(expr) {
        return Some(LiteralValue::String(text));
    }

    match expr {
        "true" => return Some(LiteralValue::Bool(true)),
        "false" => return Some(LiteralValue::Bool(false)),
        "null" => return Some(LiteralValue::Null),
        "undefined" => return Some(LiteralValue::Undefined),
        _ => {}
    }

    if let Some(value) = parse_bigint_literal(expr) {
        return Some(value);
    }

    if let Some(value) = parse_symbol_literal(expr) {
        return Some(value);
    }

    if is_integer_literal(expr) {
        if let Ok(value) = expr.parse::<i64>() {
            return Some(LiteralValue::Int(value));
        }
    }

    if is_float_literal(expr) {
        if let Ok(value) = expr.parse::<f64>() {
            return Some(LiteralValue::Float(value));
        }
    }

    if let Some(value) = parse_array_literal(expr, constants) {
        return Some(value);
    }

    if let Some(value) = parse_object_literal(expr, constants) {
        return Some(value);
    }

    None
}

fn parse_array_literal(
    expr: &str,
    constants: &BTreeMap<String, LiteralValue>,
) -> Option<LiteralValue> {
    if !(expr.starts_with('[') && expr.ends_with(']')) {
        return None;
    }

    let inner = expr[1..expr.len() - 1].trim();
    if inner.is_empty() {
        return Some(LiteralValue::Array(Vec::new()));
    }

    let mut values = Vec::new();
    for piece in split_top_level(inner, ',') {
        let value = parse_literal_value(piece.trim(), constants)?;
        values.push(value);
    }

    Some(LiteralValue::Array(values))
}

fn parse_object_literal(
    expr: &str,
    constants: &BTreeMap<String, LiteralValue>,
) -> Option<LiteralValue> {
    if !(expr.starts_with('{') && expr.ends_with('}')) {
        return None;
    }

    let inner = expr[1..expr.len() - 1].trim();
    if inner.is_empty() {
        return Some(LiteralValue::Object(Vec::new()));
    }

    let mut entries = Vec::new();
    for piece in split_top_level(inner, ',') {
        let (raw_key, raw_value) = piece.split_once(':')?;
        let key = normalize_object_key(raw_key.trim())?;
        let value = parse_literal_value(raw_value.trim(), constants)?;
        entries.push((key, value));
    }

    Some(LiteralValue::Object(entries))
}

fn normalize_object_key(raw: &str) -> Option<String> {
    if is_identifier(raw) {
        return Some(raw.to_string());
    }

    parse_string_literal(raw)
}

fn parse_string_literal(expr: &str) -> Option<String> {
    if expr.len() < 2 {
        return None;
    }

    let quote = expr.chars().next()?;
    if !matches!(quote, '\'' | '"' | '`') {
        return None;
    }

    if !expr.ends_with(quote) {
        return None;
    }

    let inner = &expr[1..expr.len() - 1];
    Some(
        inner
            .replace("\\\\", "\\")
            .replace("\\\"", "\"")
            .replace("\\\'", "\'")
            .replace("\\n", "\n")
            .replace("\\t", "\t"),
    )
}

fn parse_symbol_literal(expr: &str) -> Option<LiteralValue> {
    let inner = expr.strip_prefix("Symbol(")?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return Some(LiteralValue::Symbol(String::new()));
    }

    Some(LiteralValue::Symbol(inner.to_string()))
}

fn parse_bigint_literal(expr: &str) -> Option<LiteralValue> {
    if let Some(raw) = expr.strip_suffix('n') {
        if raw.chars().all(|ch| ch.is_ascii_digit()) {
            return Some(LiteralValue::BigInt(raw.to_string()));
        }
    }

    let inner = expr.strip_prefix("BigInt(")?.strip_suffix(')')?.trim();
    if inner.chars().all(|ch| ch.is_ascii_digit()) {
        return Some(LiteralValue::BigInt(inner.to_string()));
    }

    None
}

fn parse_zero_arg_call(line: &str) -> Option<&str> {
    let trimmed = line.trim().trim_end_matches(';').trim();
    let name = trimmed.strip_suffix("()")?.trim();
    if is_identifier(name) {
        Some(name)
    } else {
        None
    }
}

#[derive(Debug, Clone)]
enum LiteralValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
    Undefined,
    Array(Vec<LiteralValue>),
    Object(Vec<(String, LiteralValue)>),
    Symbol(String),
    BigInt(String),
}

impl LiteralValue {
    fn inferred_type(&self) -> String {
        match self {
            Self::Int(value) => infer_integer_type(*value).to_string(),
            Self::Float(_) => "f64".to_string(),
            Self::String(_) => "string".to_string(),
            Self::Bool(_) => "boolean".to_string(),
            Self::Null => "null".to_string(),
            Self::Undefined => "undefined".to_string(),
            Self::Array(values) => {
                if values.is_empty() {
                    return "any[]".to_string();
                }

                let element = merge_types(values.iter().map(|value| value.inferred_type()));
                format!("{}[]", element.unwrap_or_else(|| "any".to_string()))
            }
            Self::Object(entries) => {
                if entries.is_empty() {
                    return "{}".to_string();
                }

                let parts = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value.inferred_type()))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", parts.join(", "))
            }
            Self::Symbol(_) => "symbol".to_string(),
            Self::BigInt(_) => "bigint".to_string(),
        }
    }

    fn render(&self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Float(value) => render_float(*value),
            Self::String(value) => format!("\"{}\"", escape_string(value)),
            Self::Bool(value) => value.to_string(),
            Self::Null => "null".to_string(),
            Self::Undefined => "undefined".to_string(),
            Self::Array(values) => {
                let rendered = values.iter().map(Self::render).collect::<Vec<_>>();
                format!("[{}]", rendered.join(", "))
            }
            Self::Object(entries) => {
                let rendered = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", render_object_key(key), value.render()))
                    .collect::<Vec<_>>();
                format!("{{ {} }}", rendered.join(", "))
            }
            Self::Symbol(inner) => format!("Symbol({inner})"),
            Self::BigInt(inner) => format!("BigInt({inner})"),
        }
    }
}

fn merge_types(types: impl Iterator<Item = String>) -> Option<String> {
    let collected = types.collect::<Vec<_>>();
    if collected.is_empty() {
        return None;
    }

    if collected.iter().all(|ty| is_numeric_type(ty)) {
        if collected.iter().any(|ty| ty == "f64" || ty == "f32") {
            return Some("f64".to_string());
        }

        let mut rank = 1usize;
        for ty in &collected {
            rank = rank.max(integer_rank(ty));
        }

        return Some(
            match rank {
                1 => "i8",
                2 => "i16",
                3 => "i32",
                _ => "i64",
            }
            .to_string(),
        );
    }

    let first = collected.first()?.clone();
    if collected.iter().all(|ty| ty == &first) {
        Some(first)
    } else {
        Some("any".to_string())
    }
}

fn infer_integer_type(value: i64) -> &'static str {
    if (i8::MIN as i64..=i8::MAX as i64).contains(&value) {
        "i8"
    } else if (i16::MIN as i64..=i16::MAX as i64).contains(&value) {
        "i16"
    } else if (i32::MIN as i64..=i32::MAX as i64).contains(&value) {
        "i32"
    } else {
        "i64"
    }
}

fn render_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }

    if value.is_infinite() {
        return if value.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }

    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        value.to_string()
    }
}

fn render_object_key(key: &str) -> String {
    if is_identifier(key) {
        key.to_string()
    } else {
        format!("\"{}\"", escape_string(key))
    }
}

fn escape_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn is_type_compatible(declared: &str, inferred: &str) -> bool {
    let declared = normalize_type_name(declared);
    let inferred = normalize_type_name(inferred);

    if matches!(declared.as_str(), "any" | "unknown") {
        return true;
    }

    if declared == inferred {
        return true;
    }

    if declared == "number" && is_numeric_type(&inferred) {
        return true;
    }

    if is_numeric_type(&declared) && is_numeric_type(&inferred) {
        return true;
    }

    if declared.ends_with("[]") && inferred.ends_with("[]") {
        return is_type_compatible(
            declared.trim_end_matches("[]"),
            inferred.trim_end_matches("[]"),
        );
    }

    false
}

fn normalize_type_name(raw: &str) -> String {
    raw.trim().replace(' ', "")
}

fn is_numeric_type(ty: &str) -> bool {
    matches!(ty, "number" | "i8" | "i16" | "i32" | "i64" | "f32" | "f64")
}

fn integer_rank(ty: &str) -> usize {
    match ty {
        "i8" => 1,
        "i16" => 2,
        "i32" => 3,
        "i64" => 4,
        _ => 4,
    }
}

fn is_integer_literal(text: &str) -> bool {
    if text.is_empty() {
        return false;
    }

    let digits = text.strip_prefix('-').unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

fn is_float_literal(text: &str) -> bool {
    if !text.contains('.') {
        return false;
    }

    text.parse::<f64>().is_ok()
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

#[cfg(test)]
mod tests {
    use crate::mir::cfg::{BasicBlock, Terminator};
    use crate::mir::{MirFunction, MirModule, MirStatement};

    use super::optimize;

    #[test]
    fn deduplicates_imports_and_infers_literals() {
        let mut module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![
                        MirStatement {
                            text: "import { print } from \"rts\";".to_string(),
                        },
                        MirStatement {
                            text: "import { print } from \"rts\";".to_string(),
                        },
                        MirStatement {
                            text: "const valor = 1".to_string(),
                        },
                        MirStatement {
                            text: "const valor2 = valor".to_string(),
                        },
                    ],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = optimize(&mut module);
        assert_eq!(report.deduplicated_imports, 1);
        assert_eq!(report.inferred_declarations, 2);

        let statements = &module.functions[0].blocks[0].statements;
        assert!(
            statements
                .iter()
                .any(|stmt| stmt.text == "const valor: i8 = 1;")
        );
        assert!(
            statements
                .iter()
                .any(|stmt| stmt.text == "const valor2: i8 = 1;")
        );
    }

    #[test]
    fn inlines_trivial_zero_arg_function_alias() {
        let mut module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![
                        MirStatement {
                            text: "const valor11 = () => { console.log(\"Hello\") }".to_string(),
                        },
                        MirStatement {
                            text: "valor11()".to_string(),
                        },
                    ],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = optimize(&mut module);
        assert_eq!(report.inlined_calls, 1);

        let statements = &module.functions[0].blocks[0].statements;
        assert!(
            statements
                .iter()
                .any(|stmt| stmt.text == "console.log(\"Hello\");")
        );
    }

    #[test]
    fn flags_declared_type_mismatches() {
        let mut module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![MirStatement {
                        text: "const valor: string = 1".to_string(),
                    }],
                    terminator: Terminator::Return,
                }],
            }],
        };

        let report = optimize(&mut module);
        assert_eq!(report.type_mismatches, 1);
    }

    #[test]
    fn infers_array_and_object_shapes() {
        let mut module = MirModule {
            functions: vec![MirFunction {
                name: "main".to_string(),
                blocks: vec![BasicBlock {
                    label: "entry".to_string(),
                    statements: vec![
                        MirStatement {
                            text: "const arr = [1, 2, 3]".to_string(),
                        },
                        MirStatement {
                            text: "const obj = { nome: \"John\", idade: 30 }".to_string(),
                        },
                    ],
                    terminator: Terminator::Return,
                }],
            }],
        };

        optimize(&mut module);
        let statements = &module.functions[0].blocks[0].statements;

        assert!(
            statements
                .iter()
                .any(|stmt| stmt.text == "const arr: i8[] = [1, 2, 3];")
        );
        assert!(statements.iter().any(|stmt| stmt.text
            == "const obj: { nome: string, idade: i8 } = { nome: \"John\", idade: 30 };"));
    }
}
