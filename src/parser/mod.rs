pub mod ast;
pub mod lexer;
pub mod span;

use anyhow::Result;
use ast::{
    ClassDecl, ClassMember, ConstructorDecl, FunctionDecl, ImportDecl, InterfaceDecl, Item,
    MemberModifiers, MethodDecl, Parameter, Program, PropertyDecl, Statement, Visibility,
};
use span::{Span, Spanned};

pub fn parse_source(source: &str) -> Result<Program> {
    let _tokens = lexer::tokenize(source);

    let mut program = Program::default();
    let mut lines = source.lines().enumerate().peekable();

    while let Some((line_index, raw_line)) = lines.next() {
        let trimmed = strip_inline_comment(trim_bom(raw_line).trim());

        if trimmed.is_empty() {
            continue;
        }

        let span = Span::new(line_index + 1, 1, raw_line.len().max(1));

        if let Some(import_decl) = parse_import_decl(trimmed, span) {
            program.items.push(Item::Import(import_decl));
            continue;
        }

        let normalized = strip_export_prefix(trimmed);

        if let Some(interface_name) = normalized.strip_prefix("interface ") {
            let mut interface_decl = InterfaceDecl {
                name: parse_decl_name(interface_name),
                fields: Vec::new(),
                span,
            };

            let mut depth = brace_delta(normalized);

            while depth > 0 {
                let Some((field_line_index, field_line)) = lines.next() else {
                    break;
                };

                let field_trimmed = strip_inline_comment(trim_bom(field_line).trim());

                if depth == 1 {
                    if let Some(field) = parse_field_decl(
                        field_trimmed,
                        Span::new(field_line_index + 1, 1, field_line.len().max(1)),
                    ) {
                        interface_decl.fields.push(field);
                    }
                }

                depth += brace_delta(field_trimmed);
            }

            program.items.push(Item::Interface(interface_decl));
            continue;
        }

        if let Some(class_name) = normalized.strip_prefix("class ") {
            let mut class_decl = ClassDecl {
                name: parse_decl_name(class_name),
                members: Vec::new(),
                span,
            };

            let mut depth = brace_delta(normalized);

            while depth > 0 {
                let Some((member_line_index, member_line)) = lines.next() else {
                    break;
                };

                let member_trimmed = strip_inline_comment(trim_bom(member_line).trim());

                if depth == 1 {
                    let member_span = Span::new(member_line_index + 1, 1, member_line.len().max(1));
                    if let Some(member) = parse_class_member(member_trimmed, member_span) {
                        class_decl.members.push(member);
                    }
                }

                depth += brace_delta(member_trimmed);
            }

            program.items.push(Item::Class(class_decl));
            continue;
        }

        if let Some(rest) = normalized.strip_prefix("function ") {
            let (name, parameters, return_type) = parse_callable_signature(rest, span);
            program.items.push(Item::Function(FunctionDecl {
                name,
                parameters,
                return_type,
                body: Vec::new(),
                span,
            }));
            continue;
        }

        program
            .items
            .push(Item::Statement(Statement::Raw(Spanned::new(
                trimmed.to_string(),
                span,
            ))));
    }

    Ok(program)
}

fn trim_bom(line: &str) -> &str {
    line.trim_start_matches('\u{feff}')
}

fn strip_export_prefix(line: &str) -> &str {
    line.strip_prefix("export ").unwrap_or(line).trim_start()
}

fn strip_inline_comment(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
    .trim()
}

fn parse_import_decl(line: &str, span: Span) -> Option<ImportDecl> {
    if !line.starts_with("import ") {
        return None;
    }

    let open = line.find('{')?;
    let close = line[open + 1..].find('}')? + open + 1;
    let names = line[open + 1..close]
        .split(',')
        .filter_map(normalize_import_name)
        .collect::<Vec<_>>();

    if names.is_empty() {
        return None;
    }

    let remainder = line[close + 1..].trim();
    let from_index = remainder.find("from")?;
    let source = remainder[from_index + "from".len()..]
        .trim()
        .trim_end_matches(';')
        .trim();

    let module_name = source.trim_matches('"').trim_matches('\'');
    if module_name.is_empty() {
        return None;
    }

    Some(ImportDecl {
        names,
        from: module_name.to_string(),
        span,
    })
}

fn normalize_import_name(raw: &str) -> Option<String> {
    let mut text = raw.trim();

    if let Some(stripped) = text.strip_prefix("type ") {
        text = stripped.trim_start();
    }

    if let Some((left, _right)) = text.split_once(" as ") {
        text = left.trim();
    }

    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

fn parse_field_decl(line: &str, span: Span) -> Option<ast::FieldDecl> {
    if line.is_empty() || line.starts_with('}') {
        return None;
    }

    let content = line.trim_end_matches(';').trim();
    let mut parts = content.splitn(2, ':');
    let name = parts.next()?.trim();
    let ty = parts.next()?.trim();

    if name.is_empty() || ty.is_empty() {
        return None;
    }

    Some(ast::FieldDecl {
        name: name.to_string(),
        type_annotation: ty.to_string(),
        span,
    })
}

fn parse_class_member(line: &str, span: Span) -> Option<ClassMember> {
    if line.is_empty() || line == "}" {
        return None;
    }

    if line.starts_with("constructor") || line.starts_with("public constructor") {
        let (_, parameters, _) = parse_callable_signature(line, span);
        return Some(ClassMember::Constructor(ConstructorDecl {
            parameters,
            span,
        }));
    }

    if line.contains('(') && line.contains(')') {
        let (name, parameters, return_type, modifiers) = parse_method_signature(line, span);
        if !name.is_empty() {
            return Some(ClassMember::Method(MethodDecl {
                name,
                modifiers,
                parameters,
                return_type,
                span,
            }));
        }
    }

    parse_property_signature(line, span).map(ClassMember::Property)
}

fn parse_method_signature(
    line: &str,
    span: Span,
) -> (String, Vec<Parameter>, Option<String>, MemberModifiers) {
    let call_open = match line.find('(') {
        Some(index) => index,
        None => return (String::new(), Vec::new(), None, MemberModifiers::default()),
    };

    let call_close = match line.rfind(')') {
        Some(index) if index > call_open => index,
        _ => return (String::new(), Vec::new(), None, MemberModifiers::default()),
    };

    let head = line[..call_open].trim();
    let (modifiers, name) = parse_modifiers_and_name(head);

    let params_raw = line[call_open + 1..call_close].trim();
    let parameters = parse_parameters(params_raw, span);

    let tail = line[call_close + 1..].trim();
    let return_type = parse_return_type(tail);

    (name, parameters, return_type, modifiers)
}

fn parse_property_signature(line: &str, span: Span) -> Option<PropertyDecl> {
    let content = line.trim_end_matches(';').trim();
    if content.contains('(') {
        return None;
    }

    let mut parts = content.splitn(2, ':');
    let left = parts.next()?.trim();
    let type_annotation = parts.next().map(|value| value.trim().to_string());

    let (modifiers, name) = parse_modifiers_and_name(left);
    if name.is_empty() {
        return None;
    }

    Some(PropertyDecl {
        name,
        modifiers,
        type_annotation,
        span,
    })
}

fn parse_callable_signature(text: &str, span: Span) -> (String, Vec<Parameter>, Option<String>) {
    let line = text.trim();
    let open = match line.find('(') {
        Some(index) => index,
        None => return (parse_decl_name(line), Vec::new(), None),
    };

    let close = match line.rfind(')') {
        Some(index) if index > open => index,
        _ => return (parse_decl_name(line), Vec::new(), None),
    };

    let head = line[..open].trim();
    let name = parse_decl_name(head);

    let params = parse_parameters(line[open + 1..close].trim(), span);
    let return_type = parse_return_type(line[close + 1..].trim());

    (name, params, return_type)
}

fn parse_parameters(raw: &str, span: Span) -> Vec<Parameter> {
    if raw.is_empty() {
        return Vec::new();
    }

    split_top_level(raw, ',')
        .into_iter()
        .filter_map(|piece| parse_parameter(piece.trim(), span))
        .collect()
}

fn parse_parameter(raw: &str, span: Span) -> Option<Parameter> {
    if raw.is_empty() {
        return None;
    }

    let mut modifiers = MemberModifiers::default();
    let mut rest = raw.trim();
    let mut variadic = false;

    if let Some(stripped) = rest.strip_prefix("...") {
        variadic = true;
        rest = stripped.trim_start();
    }

    loop {
        let Some((keyword, tail)) = next_keyword(rest) else {
            break;
        };

        match keyword {
            "public" => modifiers.visibility = Some(Visibility::Public),
            "private" => modifiers.visibility = Some(Visibility::Private),
            "protected" => modifiers.visibility = Some(Visibility::Protected),
            "readonly" => modifiers.readonly = true,
            "static" => modifiers.is_static = true,
            _ => break,
        }

        rest = tail.trim_start();
    }

    let mut parts = rest.splitn(2, ':');
    let name = parts.next()?.trim();
    if name.is_empty() {
        return None;
    }

    let type_annotation = parts.next().map(|value| value.trim().to_string());

    Some(Parameter {
        name: name.to_string(),
        type_annotation,
        modifiers,
        variadic,
        span,
    })
}

fn parse_modifiers_and_name(raw: &str) -> (MemberModifiers, String) {
    let mut modifiers = MemberModifiers::default();
    let mut name_tokens = Vec::new();

    for token in raw.split_whitespace() {
        match token {
            "public" => modifiers.visibility = Some(Visibility::Public),
            "private" => modifiers.visibility = Some(Visibility::Private),
            "protected" => modifiers.visibility = Some(Visibility::Protected),
            "readonly" => modifiers.readonly = true,
            "static" => modifiers.is_static = true,
            _ => name_tokens.push(token),
        }
    }

    let name = name_tokens.last().copied().unwrap_or("").to_string();
    (modifiers, name)
}

fn parse_return_type(raw: &str) -> Option<String> {
    let content = raw.trim().trim_end_matches('{').trim();
    let stripped = content.strip_prefix(':')?.trim();
    if stripped.is_empty() {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn split_top_level(input: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut angle = 0i32;
    let mut square = 0i32;
    let mut round = 0i32;

    for (index, ch) in input.char_indices() {
        match ch {
            '<' => angle += 1,
            '>' => angle -= 1,
            '[' => square += 1,
            ']' => square -= 1,
            '(' => round += 1,
            ')' => round -= 1,
            _ => {}
        }

        if ch == separator && angle == 0 && square == 0 && round == 0 {
            parts.push(input[start..index].trim());
            start = index + ch.len_utf8();
        }
    }

    parts.push(input[start..].trim());
    parts
}

fn next_keyword(input: &str) -> Option<(&str, &str)> {
    let mut iter = input.splitn(2, char::is_whitespace);
    let first = iter.next()?;
    let rest = iter.next().unwrap_or("");
    Some((first, rest))
}

fn brace_delta(line: &str) -> i32 {
    let open = line.chars().filter(|ch| *ch == '{').count() as i32;
    let close = line.chars().filter(|ch| *ch == '}').count() as i32;
    open - close
}

fn parse_decl_name(rest: &str) -> String {
    rest.split(|c: char| c == '{' || c == '(' || c.is_whitespace())
        .find(|segment| !segment.is_empty())
        .unwrap_or("anonymous")
        .to_string()
}
