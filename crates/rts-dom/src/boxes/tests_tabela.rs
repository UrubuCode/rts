//! The anonymous TABLE family of the box tree (CSS 2.1 §17.2.1, rule 3),
//! apart from `tests.rs` because that file is past the 500-line ceiling.

use super::*;
use super::tests::no_com_id;

/// CSS 2.1 §17.2.1 rule 3: a run of consecutive table parts whose parent is
/// not a table gets ONE anonymous table around it. The whitespace between the
/// parts goes in with them; the whitespace after the last one does not.
#[test]
fn misparented_table_parts_are_wrapped_in_one_anonymous_table() {
    let dom = crate::parse_html_to_dom(
        "<div id='w'>x<div id='a' style='display:table-cell'></div> \
         <div id='b' style='display:table-row'></div> <p id='p'>y</p></div>",
    );
    let tree = build_mirror(&dom);
    let w = no_com_id(&dom, "w");
    let filhas = tree.children(tree.boxes_of(w)[0]).to_vec();
    let tabelas: Vec<_> = filhas
        .iter()
        .copied()
        .filter(|&c| matches!(tree.kind(c), BoxKind::Anonymous { role: AnonymousRole::Table, .. }))
        .collect();
    assert_eq!(tabelas.len(), 1, "one run, one table: {filhas:?}");
    let dentro: Vec<_> = tree.children(tabelas[0]).iter().filter_map(|&c| tree.node_of(c)).collect();
    assert_eq!(dentro.first(), Some(&no_com_id(&dom, "a")));
    assert_eq!(dentro.last(), Some(&no_com_id(&dom, "b")), "the trailing space stays outside");
    assert_eq!(dentro.len(), 3, "cell, the space between, row: {dentro:?}");
    assert_eq!(tree.formatting_context(&dom, tabelas[0]).inner, InnerDisplay::Table);
}

/// The refusals: a flex container blockifies its children (a `table-cell`
/// there is a flex item), and a table wraps its own misparented children in
/// `table/grid.rs`. Neither gets an anonymous table from the tree.
#[test]
fn flex_containers_and_tables_do_not_get_an_anonymous_table() {
    for html in [
        "<div id='w' style='display:flex'><div style='display:table-cell'></div></div>",
        "<div id='w' style='display:table'><div style='display:table-cell'></div></div>",
    ] {
        let dom = crate::parse_html_to_dom(html);
        let tree = build_mirror(&dom);
        let w = no_com_id(&dom, "w");
        assert!(
            tree.children(tree.boxes_of(w)[0])
                .iter()
                .all(|&c| !matches!(tree.kind(c), BoxKind::Anonymous { .. })),
            "{html}"
        );
    }
}
