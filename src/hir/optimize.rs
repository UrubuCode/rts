use std::collections::{BTreeMap, BTreeSet};

use crate::compile_options::FrontendMode;

use super::nodes::{HirImport, HirItem, HirModule};

#[derive(Debug, Clone, Copy, Default)]
pub struct HirOptimizationReport {
    pub deduplicated_imports: usize,
    pub removed_noops: usize,
    pub simplified_statements: usize,
    pub inlined_calls: usize,
}

pub fn optimize(module: &mut HirModule) -> HirOptimizationReport {
    optimize_with_mode(module, FrontendMode::Native)
}

pub fn optimize_with_mode(module: &mut HirModule, mode: FrontendMode) -> HirOptimizationReport {
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

                    if let Some(folded) = fold_numeric_expression(&expr) {
                        if folded != expr.trim() {
                            report.simplified_statements += 1;
                        }
                        expr = folded;
                        inferred_type = infer_literal_type(&expr);
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

                    let annotation = declaration
                        .explicit_type
                        .or_else(|| annotation_for_mode(mode, inferred_type));
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
        let value = text.parse::<i128>().ok()?;
        return Some(if (i8::MIN as i128..=i8::MAX as i128).contains(&value) {
            "i8"
        } else if (i16::MIN as i128..=i16::MAX as i128).contains(&value) {
            "i16"
        } else if (i32::MIN as i128..=i32::MAX as i128).contains(&value) {
            "i32"
        } else if (i64::MIN as i128..=i64::MAX as i128).contains(&value) {
            "i64"
        } else {
            "f64"
        });
    }

    if text.contains('.') && text.parse::<f64>().is_ok() {
        return Some("f64");
    }

    None
}

fn annotation_for_mode(mode: FrontendMode, inferred_type: Option<String>) -> Option<String> {
    match mode {
        FrontendMode::Native => inferred_type,
        FrontendMode::Compat => inferred_type.or_else(|| Some("any".to_string())),
    }
}

fn fold_numeric_expression(expr: &str) -> Option<String> {
    let mut parser = NumericExprParser::new(expr);
    let value = parser.parse_expression()?;
    parser.skip_ws();
    if !parser.is_end() {
        return None;
    }
    Some(value.to_source())
}

#[derive(Debug, Clone, Copy)]
enum NumericValue {
    Int(i128),
    Float(f64),
}

impl NumericValue {
    fn add(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (Self::Int(left), Self::Int(right)) => left.checked_add(right).map(Self::Int),
            (left, right) => Some(Self::Float(left.as_f64()? + right.as_f64()?)),
        }
    }

    fn sub(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (Self::Int(left), Self::Int(right)) => left.checked_sub(right).map(Self::Int),
            (left, right) => Some(Self::Float(left.as_f64()? - right.as_f64()?)),
        }
    }

    fn mul(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (Self::Int(left), Self::Int(right)) => left.checked_mul(right).map(Self::Int),
            (left, right) => Some(Self::Float(left.as_f64()? * right.as_f64()?)),
        }
    }

    fn div(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (_, Self::Int(0)) => None,
            (_, Self::Float(value)) if value == 0.0 => None,
            (Self::Int(left), Self::Int(right)) => {
                if left % right == 0 {
                    Some(Self::Int(left / right))
                } else {
                    Some(Self::Float((left as f64) / (right as f64)))
                }
            }
            (left, right) => Some(Self::Float(left.as_f64()? / right.as_f64()?)),
        }
    }

    fn rem(self, rhs: Self) -> Option<Self> {
        match (self, rhs) {
            (_, Self::Int(0)) => None,
            (_, Self::Float(value)) if value == 0.0 => None,
            (Self::Int(left), Self::Int(right)) => Some(Self::Int(left % right)),
            (left, right) => Some(Self::Float(left.as_f64()? % right.as_f64()?)),
        }
    }

    fn neg(self) -> Option<Self> {
        match self {
            Self::Int(value) => value.checked_neg().map(Self::Int),
            Self::Float(value) => Some(Self::Float(-value)),
        }
    }

    fn as_f64(self) -> Option<f64> {
        match self {
            Self::Int(value) => Some(value as f64),
            Self::Float(value) if value.is_finite() => Some(value),
            Self::Float(_) => None,
        }
    }

    fn to_source(self) -> String {
        match self {
            Self::Int(value) => value.to_string(),
            Self::Float(value) => format_float(value),
        }
    }
}

fn format_float(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }

    let mut text = format!("{value:.12}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.push('0');
    }
    text
}

#[derive(Debug)]
struct NumericExprParser<'a> {
    input: &'a str,
    cursor: usize,
}

impl<'a> NumericExprParser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, cursor: 0 }
    }

    fn parse_expression(&mut self) -> Option<NumericValue> {
        self.parse_add_sub()
    }

    fn parse_add_sub(&mut self) -> Option<NumericValue> {
        let mut value = self.parse_mul_div()?;

        loop {
            self.skip_ws();
            if self.consume('+') {
                value = value.add(self.parse_mul_div()?)?;
            } else if self.consume('-') {
                value = value.sub(self.parse_mul_div()?)?;
            } else {
                break;
            }
        }

        Some(value)
    }

    fn parse_mul_div(&mut self) -> Option<NumericValue> {
        let mut value = self.parse_unary()?;

        loop {
            self.skip_ws();
            if self.consume('*') {
                value = value.mul(self.parse_unary()?)?;
            } else if self.consume('/') {
                value = value.div(self.parse_unary()?)?;
            } else if self.consume('%') {
                value = value.rem(self.parse_unary()?)?;
            } else {
                break;
            }
        }

        Some(value)
    }

    fn parse_unary(&mut self) -> Option<NumericValue> {
        self.skip_ws();

        if self.consume('+') {
            return self.parse_unary();
        }

        if self.consume('-') {
            return self.parse_unary()?.neg();
        }

        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<NumericValue> {
        self.skip_ws();

        if self.consume('(') {
            let value = self.parse_expression()?;
            self.skip_ws();
            if !self.consume(')') {
                return None;
            }
            return Some(value);
        }

        self.parse_number()
    }

    fn parse_number(&mut self) -> Option<NumericValue> {
        self.skip_ws();
        let start = self.cursor;
        let mut seen_dot = false;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.bump();
                continue;
            }
            if ch == '.' && !seen_dot {
                seen_dot = true;
                self.bump();
                continue;
            }
            break;
        }

        if self.cursor == start {
            return None;
        }

        let text = &self.input[start..self.cursor];
        if seen_dot {
            text.parse::<f64>().ok().map(NumericValue::Float)
        } else {
            text.parse::<i128>().ok().map(NumericValue::Int)
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.bump();
            } else {
                break;
            }
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(ch) = self.peek() {
            self.cursor += ch.len_utf8();
        }
    }

    fn is_end(&self) -> bool {
        self.cursor >= self.input.len()
    }
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
    use crate::compile_options::FrontendMode;

    use super::{HirItem, HirModule, optimize, optimize_with_mode};

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

    #[test]
    fn folds_arithmetic_expression_and_narrows_integer_type() {
        let mut module = HirModule {
            items: vec![HirItem::Statement(
                "const valor = 2 * 60 * 60 * 1000;".to_string(),
            )],
            ..Default::default()
        };

        let _ = optimize(&mut module);

        assert!(module.items.iter().any(
            |item| matches!(item, HirItem::Statement(text) if text == "const valor: i32 = 7200000;")
        ));
    }

    #[test]
    fn compat_mode_falls_back_to_dynamic_type_when_not_inferable() {
        let mut module = HirModule {
            items: vec![HirItem::Statement(
                "const valor = chamarAlgo();".to_string(),
            )],
            ..Default::default()
        };

        let _ = optimize_with_mode(&mut module, FrontendMode::Compat);

        assert!(module.items.iter().any(
            |item| matches!(item, HirItem::Statement(text) if text == "const valor: any = chamarAlgo();")
        ));
    }
}
