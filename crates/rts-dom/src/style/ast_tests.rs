use super::*;

#[test]
fn stylesheet_expoe_ast_original_e_declaracoes_desconhecidas() {
    let mut sheet = Stylesheet::new();
    let css = "/* preservado */ .a { color: red; future-prop: mystery(1, 2); }";
    sheet.append_css(css);

    assert_eq!(sheet.syntax().len(), 1);
    assert_eq!(sheet.syntax()[0].to_css(), css);
    assert!(sheet.syntax()[0]
        .items
        .iter()
        .any(|item| item.prelude_css().contains(".a")));
    assert!(sheet.diagnostics().is_empty());
}

#[test]
fn stylesheet_expoe_diagnostico_de_bloco_incompleto() {
    let mut sheet = Stylesheet::new();
    sheet.append_css(".broken { color: red");
    assert!(sheet
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.message.contains("fecho")));
}
