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

    // `!rule.is_ua`: `sheet.rules` agora inclui a folha de UA (lote I), que
    // tem várias regras de UM compound (`html`, `body`, `p`, …) — sem este
    // filtro o `.find` achava a primeira delas em vez da `.a` deste teste.
    let rule = sheet
        .rules
        .iter()
        .find(|rule| !rule.is_ua && rule.selector.compounds.len() == 1)
        .unwrap();
    assert!(
        rule.source_declarations
            .iter()
            .any(|declaration| declaration.name == "future-prop")
    );
    assert!(
        rule.specified
            .declarations()
            .iter()
            .any(|declaration| declaration.name_raw.contains("future-prop"))
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

#[test]
fn specified_style_preserva_ordem_e_importancia() {
    let ast = StylesheetAst::parse(".a { COLOR: red; width: calc(50% + 2px) !important; }");
    let AstItem::QualifiedRule { block, .. } = &ast.items[0] else {
        panic!("regra qualificada esperada");
    };
    let specified = SpecifiedStyle::from_block(block);

    assert_eq!(specified.declarations().len(), 2);
    assert_eq!(specified.declarations()[0].name_raw.trim(), "COLOR");
    assert!(!specified.declarations()[0].important);
    assert!(specified.declarations()[1].important);
    assert!(specified.to_css().contains("COLOR: red;"));
    assert!(specified
        .to_css()
        .contains("width: calc(50% + 2px) !important;"));
}

#[test]
fn inline_specified_usa_a_mesma_fronteira_do_stylesheet() {
    let specified = parse_inline_specified("color: var(--brand); --brand: rebeccapurple");
    assert_eq!(specified.declarations().len(), 2);
    assert_eq!(specified.declarations()[0].name, "color");
    assert_eq!(specified.declarations()[1].name, "--brand");
    assert!(specified.to_css().contains("color: var(--brand);"));
}


#[test]
fn stylesheet_cssom_insert_delete_reconstroi_o_lowering() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(".a { color: red }");
    assert_eq!(sheet.computed_for("div", None, &["a"]).normal.color, Some(0xff0000ff));

    assert_eq!(sheet.insert_rule(1, ".a { color: blue }").unwrap(), 1);
    assert_eq!(sheet.computed_for("div", None, &["a"]).normal.color, Some(0x0000ffff));
    assert_eq!(sheet.syntax().len(), 2);

    assert!(!sheet.delete_rule(99));
    assert!(sheet.delete_rule(0));
    assert_eq!(sheet.computed_for("div", None, &["a"]).normal.color, Some(0x0000ffff));
    assert_eq!(sheet.syntax().len(), 1);
}
