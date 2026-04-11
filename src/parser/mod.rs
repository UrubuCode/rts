pub mod ast;
pub mod lexer;
pub mod span;

use anyhow::{Result, anyhow};
use swc_common::{FileName, SourceMap, SourceMapper, Span as SwcSpan, Spanned, sync::Lrc};
use swc_ecma_ast::{
    Accessibility, Class as SwcClass, ClassDecl as SwcClassDecl, ClassMember as SwcClassMember,
    Decl, DefaultDecl, Expr, FnDecl as SwcFnDecl, Function as SwcFunction,
    ImportDecl as SwcImportDecl, ImportSpecifier, Lit, ModuleDecl, ModuleExportName, ModuleItem,
    Param as SwcParam, ParamOrTsParamProp, Pat, Program as SwcProgram, PropName, Stmt,
    TsInterfaceDecl as SwcTsInterfaceDecl, TsParamProp, TsParamPropParam, TsTypeAnn, TsTypeElement,
};
use swc_ecma_parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};

use crate::compile_options::FrontendMode;

use ast::{
    ClassDecl, ClassMember, ConstructorDecl, FieldDecl, FunctionDecl, ImportDecl, InterfaceDecl,
    Item, MemberModifiers, MethodDecl, Parameter, Program, PropertyDecl, Statement, Visibility,
};
use span::{Position, Span, Spanned as LocalSpanned};

pub fn parse_source(source: &str) -> Result<Program> {
    parse_source_with_mode(source, FrontendMode::Native)
}

pub fn parse_source_with_mode(source: &str, mode: FrontendMode) -> Result<Program> {
    let syntax_order = match mode {
        FrontendMode::Native => [ts_syntax(), es_syntax()],
        FrontendMode::Compat => [es_syntax(), ts_syntax()],
    };

    let mut first_error = None::<String>;

    for syntax in syntax_order {
        match parse_with_syntax(source, syntax) {
            Ok(program) => return Ok(program),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error.to_string());
                }
            }
        }
    }

    Err(anyhow!(
        "failed to parse source in {} mode: {}",
        mode,
        first_error.unwrap_or_else(|| "unknown parser error".to_string())
    ))
}

fn parse_with_syntax(source: &str, syntax: Syntax) -> Result<Program> {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Lrc::new(FileName::Custom("rts-input.ts".into())),
        source.to_string(),
    );

    let lexer = Lexer::new(syntax, Default::default(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);

    let parsed = parser
        .parse_program()
        .map_err(|error| anyhow!(format_parser_error(&cm, &error)))?;

    if let Some(error) = parser.take_errors().into_iter().next() {
        return Err(anyhow!(format_parser_error(&cm, &error)));
    }

    Ok(lower_program(&cm, &parsed))
}

fn format_parser_error(cm: &Lrc<SourceMap>, error: &swc_ecma_parser::error::Error) -> String {
    let message = error.kind().msg();
    let span = error.span();

    if span.is_dummy() {
        return message.into_owned();
    }

    let loc = cm.lookup_char_pos(span.lo());
    format!(
        "{} at {}:{}",
        message,
        loc.line,
        loc.col_display.saturating_add(1)
    )
}

fn lower_program(cm: &Lrc<SourceMap>, source: &SwcProgram) -> Program {
    let mut program = Program::default();

    match source {
        SwcProgram::Module(module) => {
            for item in &module.body {
                lower_module_item(cm, item, &mut program.items);
            }
        }
        SwcProgram::Script(script) => {
            for stmt in &script.body {
                lower_stmt(cm, stmt, &mut program.items);
            }
        }
    }

    program
}

fn lower_module_item(cm: &Lrc<SourceMap>, item: &ModuleItem, out: &mut Vec<Item>) {
    match item {
        ModuleItem::ModuleDecl(decl) => lower_module_decl(cm, decl, out),
        ModuleItem::Stmt(stmt) => lower_stmt(cm, stmt, out),
    }
}

fn lower_module_decl(cm: &Lrc<SourceMap>, decl: &ModuleDecl, out: &mut Vec<Item>) {
    match decl {
        ModuleDecl::Import(import_decl) => {
            out.push(Item::Import(lower_import_decl(cm, import_decl)));
        }
        ModuleDecl::ExportDecl(export_decl) => {
            lower_decl(cm, &export_decl.decl, out);
        }
        ModuleDecl::ExportDefaultDecl(default_decl) => match &default_decl.decl {
            DefaultDecl::Class(class_expr) => {
                if let Some(name) = class_expr.ident.as_ref().map(|ident| ident.sym.to_string()) {
                    out.push(Item::Class(lower_class(
                        cm,
                        &name,
                        &class_expr.class,
                        class_expr.span(),
                    )));
                } else {
                    push_raw_statement(cm, decl.span(), out);
                }
            }
            DefaultDecl::Fn(fn_expr) => {
                if let Some(name) = fn_expr.ident.as_ref().map(|ident| ident.sym.to_string()) {
                    out.push(Item::Function(lower_function(
                        cm,
                        &name,
                        &fn_expr.function,
                        fn_expr.function.span,
                    )));
                } else {
                    push_raw_statement(cm, decl.span(), out);
                }
            }
            DefaultDecl::TsInterfaceDecl(interface_decl) => {
                out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
            }
        },
        _ => push_raw_statement(cm, decl.span(), out),
    }
}

fn lower_stmt(cm: &Lrc<SourceMap>, stmt: &Stmt, out: &mut Vec<Item>) {
    match stmt {
        Stmt::Decl(decl) => lower_decl(cm, decl, out),
        _ => push_raw_statement(cm, stmt.span(), out),
    }
}

fn lower_decl(cm: &Lrc<SourceMap>, decl: &Decl, out: &mut Vec<Item>) {
    match decl {
        Decl::Class(class_decl) => {
            out.push(Item::Class(lower_class_decl(cm, class_decl)));
        }
        Decl::Fn(fn_decl) => {
            out.push(Item::Function(lower_fn_decl(cm, fn_decl)));
        }
        Decl::TsInterface(interface_decl) => {
            out.push(Item::Interface(lower_interface_decl(cm, interface_decl)));
        }
        _ => push_raw_statement(cm, decl.span(), out),
    }
}

fn lower_import_decl(cm: &Lrc<SourceMap>, import_decl: &SwcImportDecl) -> ImportDecl {
    let mut names = Vec::new();
    let mut default_name = None;

    for specifier in &import_decl.specifiers {
        match specifier {
            ImportSpecifier::Named(named) => {
                let name = if let Some(imported) = &named.imported {
                    module_export_name(imported)
                } else {
                    named.local.sym.to_string()
                };
                names.push(name);
            }
            ImportSpecifier::Default(def) => {
                default_name = Some(def.local.sym.to_string());
            }
            ImportSpecifier::Namespace(_) => {}
        }
    }

    ImportDecl {
        names,
        default_name,
        from: import_decl.src.value.to_string_lossy().to_string(),
        span: convert_span(cm, import_decl.span),
    }
}

fn lower_interface_decl(cm: &Lrc<SourceMap>, interface_decl: &SwcTsInterfaceDecl) -> InterfaceDecl {
    let mut fields = Vec::new();

    for member in &interface_decl.body.body {
        if let TsTypeElement::TsPropertySignature(property) = member {
            if let Some(name) = property_key_name(&property.key, cm) {
                let field = FieldDecl {
                    name,
                    type_annotation: property
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation))
                        .unwrap_or_else(|| "any".to_string()),
                    span: convert_span(cm, property.span),
                };
                fields.push(field);
            }
        }
    }

    InterfaceDecl {
        name: interface_decl.id.sym.to_string(),
        fields,
        span: convert_span(cm, interface_decl.span),
    }
}

fn lower_class_decl(cm: &Lrc<SourceMap>, class_decl: &SwcClassDecl) -> ClassDecl {
    lower_class(
        cm,
        &class_decl.ident.sym.to_string(),
        &class_decl.class,
        class_decl.class.span,
    )
}

fn lower_class(cm: &Lrc<SourceMap>, name: &str, class: &SwcClass, span: SwcSpan) -> ClassDecl {
    let mut members = Vec::new();

    for member in &class.body {
        match member {
            SwcClassMember::Constructor(constructor) => {
                let parameters = constructor
                    .params
                    .iter()
                    .filter_map(|parameter| lower_constructor_param(cm, parameter))
                    .collect::<Vec<_>>();

                members.push(ClassMember::Constructor(ConstructorDecl {
                    parameters,
                    span: convert_span(cm, constructor.span),
                }));
            }
            SwcClassMember::Method(method) => {
                let name = prop_name_to_string(&method.key, cm);
                if name.is_empty() {
                    continue;
                }

                let parameters = method
                    .function
                    .params
                    .iter()
                    .filter_map(|parameter| lower_param(cm, parameter, MemberModifiers::default()))
                    .collect::<Vec<_>>();

                members.push(ClassMember::Method(MethodDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(method.accessibility),
                        readonly: false,
                        is_static: method.is_static,
                    },
                    parameters,
                    return_type: method
                        .function
                        .return_type
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    span: convert_span(cm, method.span),
                }));
            }
            SwcClassMember::PrivateMethod(method) => {
                let parameters = method
                    .function
                    .params
                    .iter()
                    .filter_map(|parameter| lower_param(cm, parameter, MemberModifiers::default()))
                    .collect::<Vec<_>>();

                members.push(ClassMember::Method(MethodDecl {
                    name: format!("#{}", method.key.name),
                    modifiers: MemberModifiers {
                        visibility: Some(Visibility::Private),
                        readonly: false,
                        is_static: method.is_static,
                    },
                    parameters,
                    return_type: method
                        .function
                        .return_type
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    span: convert_span(cm, method.span),
                }));
            }
            SwcClassMember::ClassProp(prop) => {
                let name = prop_name_to_string(&prop.key, cm);
                if name.is_empty() {
                    continue;
                }

                members.push(ClassMember::Property(PropertyDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(prop.accessibility),
                        readonly: prop.readonly,
                        is_static: prop.is_static,
                    },
                    type_annotation: prop
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    span: convert_span(cm, prop.span),
                }));
            }
            SwcClassMember::PrivateProp(prop) => {
                members.push(ClassMember::Property(PropertyDecl {
                    name: format!("#{}", prop.key.name),
                    modifiers: MemberModifiers {
                        visibility: Some(Visibility::Private),
                        readonly: prop.readonly,
                        is_static: prop.is_static,
                    },
                    type_annotation: prop
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    span: convert_span(cm, prop.span),
                }));
            }
            SwcClassMember::AutoAccessor(accessor) => {
                let name = key_to_string(&accessor.key, cm);
                if name.is_empty() {
                    continue;
                }

                members.push(ClassMember::Property(PropertyDecl {
                    name,
                    modifiers: MemberModifiers {
                        visibility: map_accessibility(accessor.accessibility),
                        readonly: false,
                        is_static: accessor.is_static,
                    },
                    type_annotation: accessor
                        .type_ann
                        .as_ref()
                        .map(|annotation| normalize_type_annotation(cm, annotation)),
                    span: convert_span(cm, accessor.span),
                }));
            }
            _ => {}
        }
    }

    ClassDecl {
        name: name.to_string(),
        members,
        span: convert_span(cm, span),
    }
}

fn lower_fn_decl(cm: &Lrc<SourceMap>, fn_decl: &SwcFnDecl) -> FunctionDecl {
    lower_function(
        cm,
        &fn_decl.ident.sym.to_string(),
        &fn_decl.function,
        fn_decl.function.span,
    )
}

fn lower_function(
    cm: &Lrc<SourceMap>,
    name: &str,
    function: &SwcFunction,
    span: SwcSpan,
) -> FunctionDecl {
    let parameters = function
        .params
        .iter()
        .filter_map(|parameter| lower_param(cm, parameter, MemberModifiers::default()))
        .collect::<Vec<_>>();

    let body = function
        .body
        .as_ref()
        .map(|body| {
            body.stmts
                .iter()
                .filter_map(|stmt| raw_statement(cm, stmt.span()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    FunctionDecl {
        name: name.to_string(),
        parameters,
        return_type: function
            .return_type
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        body,
        span: convert_span(cm, span),
    }
}

fn lower_constructor_param(
    cm: &Lrc<SourceMap>,
    parameter: &ParamOrTsParamProp,
) -> Option<Parameter> {
    match parameter {
        ParamOrTsParamProp::Param(param) => lower_param(cm, param, MemberModifiers::default()),
        ParamOrTsParamProp::TsParamProp(param_prop) => lower_ts_param_prop(cm, param_prop),
    }
}

fn lower_ts_param_prop(cm: &Lrc<SourceMap>, param_prop: &TsParamProp) -> Option<Parameter> {
    let modifiers = MemberModifiers {
        visibility: map_accessibility(param_prop.accessibility),
        readonly: param_prop.readonly,
        is_static: false,
    };

    match &param_prop.param {
        TsParamPropParam::Ident(binding) => Some(Parameter {
            name: binding.id.sym.to_string(),
            type_annotation: binding
                .type_ann
                .as_ref()
                .map(|annotation| normalize_type_annotation(cm, annotation)),
            modifiers,
            variadic: false,
            span: convert_span(cm, param_prop.span),
        }),
        TsParamPropParam::Assign(assign) => Some(Parameter {
            name: pat_name(&assign.left, cm).unwrap_or_else(|| "param".to_string()),
            type_annotation: pat_type_annotation(cm, &assign.left),
            modifiers,
            variadic: false,
            span: convert_span(cm, param_prop.span),
        }),
    }
}

fn lower_param(
    cm: &Lrc<SourceMap>,
    param: &SwcParam,
    modifiers: MemberModifiers,
) -> Option<Parameter> {
    let name = pat_name(&param.pat, cm)?;
    let variadic = matches!(param.pat, Pat::Rest(_));
    let type_annotation = pat_type_annotation(cm, &param.pat);

    Some(Parameter {
        name,
        type_annotation,
        modifiers,
        variadic,
        span: convert_span(cm, param.span),
    })
}

fn pat_name(pat: &Pat, cm: &Lrc<SourceMap>) -> Option<String> {
    match pat {
        Pat::Ident(ident) => Some(ident.id.sym.to_string()),
        Pat::Assign(assign) => pat_name(&assign.left, cm),
        Pat::Rest(rest) => pat_name(&rest.arg, cm),
        Pat::Expr(expr) => match &**expr {
            Expr::Ident(ident) => Some(ident.sym.to_string()),
            _ => span_snippet(cm, expr.span()),
        },
        _ => span_snippet(cm, pat.span()),
    }
}

fn pat_type_annotation(cm: &Lrc<SourceMap>, pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) => ident
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Array(array) => array
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Object(object) => object
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Rest(rest) => rest
            .type_ann
            .as_ref()
            .map(|annotation| normalize_type_annotation(cm, annotation)),
        Pat::Assign(assign) => pat_type_annotation(cm, &assign.left),
        _ => None,
    }
}

fn normalize_type_annotation(cm: &Lrc<SourceMap>, annotation: &TsTypeAnn) -> String {
    let snippet = span_snippet(cm, annotation.span())
        .unwrap_or_else(|| "any".to_string())
        .trim()
        .to_string();

    let stripped = snippet
        .strip_prefix(':')
        .map(str::trim)
        .unwrap_or(&snippet)
        .to_string();

    if stripped.is_empty() {
        "any".to_string()
    } else {
        stripped
    }
}

fn property_key_name(key: &Expr, cm: &Lrc<SourceMap>) -> Option<String> {
    match key {
        Expr::Ident(ident) => Some(ident.sym.to_string()),
        Expr::Lit(Lit::Str(text)) => Some(text.value.to_string_lossy().to_string()),
        Expr::Lit(Lit::Num(number)) => Some(number.value.to_string()),
        Expr::Lit(Lit::BigInt(number)) => Some(number.value.to_string()),
        _ => span_snippet(cm, key.span()),
    }
}

fn prop_name_to_string(name: &PropName, cm: &Lrc<SourceMap>) -> String {
    match name {
        PropName::Ident(ident) => ident.sym.to_string(),
        PropName::Str(text) => text.value.to_string_lossy().to_string(),
        PropName::Num(number) => number.value.to_string(),
        PropName::BigInt(number) => number.value.to_string(),
        PropName::Computed(computed) => span_snippet(cm, computed.expr.span()).unwrap_or_default(),
    }
}

fn key_to_string(key: &swc_ecma_ast::Key, cm: &Lrc<SourceMap>) -> String {
    match key {
        swc_ecma_ast::Key::Private(name) => format!("#{}", name.name),
        swc_ecma_ast::Key::Public(name) => prop_name_to_string(name, cm),
    }
}

fn module_export_name(name: &ModuleExportName) -> String {
    match name {
        ModuleExportName::Ident(ident) => ident.sym.to_string(),
        ModuleExportName::Str(value) => value.value.to_string_lossy().to_string(),
    }
}

fn map_accessibility(accessibility: Option<Accessibility>) -> Option<Visibility> {
    match accessibility {
        Some(Accessibility::Public) => Some(Visibility::Public),
        Some(Accessibility::Protected) => Some(Visibility::Protected),
        Some(Accessibility::Private) => Some(Visibility::Private),
        None => None,
    }
}

fn push_raw_statement(cm: &Lrc<SourceMap>, span: SwcSpan, out: &mut Vec<Item>) {
    if let Some(statement) = raw_statement(cm, span) {
        out.push(Item::Statement(statement));
    }
}

fn raw_statement(cm: &Lrc<SourceMap>, span: SwcSpan) -> Option<Statement> {
    let snippet = span_snippet(cm, span)?;
    let text = snippet.trim();
    if text.is_empty() {
        return None;
    }

    Some(Statement::Raw(LocalSpanned::new(
        text.to_string(),
        convert_span(cm, span),
    )))
}

fn span_snippet(cm: &Lrc<SourceMap>, span: SwcSpan) -> Option<String> {
    if span.is_dummy() {
        return None;
    }
    cm.span_to_snippet(span).ok()
}

fn convert_span(cm: &Lrc<SourceMap>, span: SwcSpan) -> Span {
    if span.is_dummy() {
        return Span::default();
    }

    let start = cm.lookup_char_pos(span.lo());
    let end = cm.lookup_char_pos(span.hi());

    Span {
        start: Position {
            line: start.line,
            column: start.col_display.saturating_add(1),
        },
        end: Position {
            line: end.line,
            column: end.col_display.saturating_add(1),
        },
    }
}

fn ts_syntax() -> Syntax {
    Syntax::Typescript(TsSyntax {
        tsx: false,
        decorators: true,
        ..Default::default()
    })
}

fn es_syntax() -> Syntax {
    Syntax::Es(EsSyntax {
        jsx: false,
        decorators: true,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::ast::Item;
    use super::{parse_source, parse_source_with_mode};
    use crate::compile_options::FrontendMode;

    #[test]
    fn parses_typescript_module_items_into_internal_ast() {
        let source = r#"
            import { print } from "rts";

            interface Teste {
                valor: i32;
            }

            class A {
                private readonly x: i8;
                constructor(public value: i16) {}
                run(): void {}
            }

            function main(x: i8): i32 {
                return x;
            }

            const valor = 2 * 60 * 60 * 1000;
        "#;

        let program = parse_source(source).expect("parser should accept valid TS");
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Import(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Interface(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Class(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Function(_)))
        );
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Statement(_)))
        );
    }

    #[test]
    fn compat_mode_parses_plain_javascript() {
        let source = "const valor = 1 + 2;";
        let program = parse_source_with_mode(source, FrontendMode::Compat)
            .expect("compat mode should parse plain JS");
        assert!(!program.items.is_empty());
    }

    #[test]
    fn compat_mode_falls_back_to_typescript_when_needed() {
        let source = "const valor: i8 = 42;";
        let program = parse_source_with_mode(source, FrontendMode::Compat)
            .expect("compat mode should fallback to TS parser");
        assert!(
            program
                .items
                .iter()
                .any(|item| matches!(item, Item::Statement(_)))
        );
    }
}
