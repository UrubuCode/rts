use crate::parse_html_to_dom;

#[test]
fn cursor_url_relativa_resolve_contra_a_pagina_da_fixture() {
    const HTML: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/css/claude-cursor-pointer-events.html"
    ));
    let dom = parse_html_to_dom(HTML);
    let id = dom.query("#url-fallback").expect("div com cursor");
    assert_eq!(
        dom.computed_property_at(id, "cursor", "http://127.0.0.1:8731/claude-cursor-pointer-events.html"),
        "url(\"http://127.0.0.1:8731/x.png\"), auto"
    );
}
