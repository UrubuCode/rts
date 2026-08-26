use super::*;

#[test]
fn stylesheet_expoe_ast_original_e_declaracoes_desconhecidas() {
    let mut sheet = Stylesheet::new();
    let css = "/* preservado */ .a { color: red; future-prop: mystery(1, 2); }";
    sheet.append_css(css);

    assert_eq!(sheet.syntax().len(), 1);
    assert_eq!(sheet.syntax()[0].to_css(), css);
    assert!(
        sheet.syntax()[0]
            .items
            .iter()
            .any(|item| item.prelude_css().contains(".a"))
    );
    assert!(sheet.diagnostics().is_empty());

    let rule = sheet
        .rules
        .iter()
        .find(|rule| rule.selector.compounds.len() == 1)
        .unwrap();
    assert!(
        rule.source_declarations
            .iter()
            .any(|declaration| declaration.name == "future-prop")
    );
}

#[test]
fn stylesheet_expoe_diagnostico_de_bloco_incompleto() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(".broken { color: red");
    assert!(
        sheet
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.message.contains("fecho"))
    );
}

#[test]
fn ast_preserva_at_rule_desconhecido_e_keyframes_recursivo() {
    let mut sheet = Stylesheet::new();
    let css = "@future feature-x; @keyframes fade { from { opacity: 0 } to { opacity: 1 } }";
    sheet.append_css(css);

    let ast = &sheet.syntax()[0];
    assert!(ast.items.iter().any(|item| matches!(
        item,
        AstItem::AtRule { name, block: None, .. } if name == "future"
    )));
    let keyframes = ast.items.iter().find_map(|item| match item {
        AstItem::AtRule {
            name,
            block: Some(block),
            ..
        } if name == "keyframes" => Some(block),
        _ => None,
    });
    assert!(keyframes.and_then(|block| block.nested.as_ref()).is_some());
    assert_eq!(
        sheet.keyframes.get("fade").map(|value| value.stops.len()),
        Some(2)
    );
}

#[test]
fn ast_reporta_funcao_incompleta_com_span() {
    let ast = StylesheetAst::parse(".a { width: calc(100% - 2px; }");
    let diagnostic = ast
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("função"))
        .expect("diagnóstico da função");
    assert!(diagnostic.span.start < diagnostic.span.end);
}
