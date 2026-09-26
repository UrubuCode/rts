//! The protocol's shape over a real parsed page — what DevTools reads.

use serde_json::{Value, json};

use super::handle;

const PAGE: &str = r#"<!doctype html><html><head><style>
.card { color: rgb(255, 0, 0); padding: 4px }
#main .card { margin-top: 8px !important }
</style></head><body>
  <div id="main"><p class="card" style="font-size: 20px">hi</p></div>
</body></html>"#;

/// Parses `PAGE` into this thread's store and answers its handle.
fn open() -> u64 {
    rts_dom::store::insert(rts_dom::parse_html_to_dom(PAGE))
}

fn call(method: &str, params: Value) -> Value {
    handle(method, &params).expect("a document method").expect(method).0
}

/// Finds the first node named `name` in a `DOM.Node` tree.
fn find<'a>(node: &'a Value, name: &str) -> Option<&'a Value> {
    if node["nodeName"] == name {
        return Some(node);
    }
    node["children"].as_array()?.iter().find_map(|child| find(child, name))
}

#[test]
fn get_document_answers_the_tree_with_ids_names_and_children() {
    open();
    let answer = call("DOM.getDocument", json!({"depth": -1}));
    let root = &answer["root"];
    assert_eq!(root["nodeName"], "#document");
    assert_eq!(root["nodeType"], 9);
    assert!(root["nodeId"].as_u64().unwrap() >= 1);
    let html = find(root, "HTML").expect("an HTML element");
    assert_eq!(html["nodeType"], 1);
    assert_eq!(html["localName"], "html");
    let p = find(root, "P").expect("the paragraph");
    assert_eq!(p["attributes"], json!(["class", "card", "style", "font-size: 20px"]));
    // Whitespace-only text is not shown; the paragraph's own text is.
    let text = &p["children"][0];
    assert_eq!(text["nodeName"], "#text");
    assert_eq!(text["nodeValue"], "hi");
    // Ids are stable: asking again answers the same number for the same node.
    let again = call("DOM.getDocument", json!({"depth": -1}));
    assert_eq!(find(&again["root"], "P").unwrap()["nodeId"], p["nodeId"]);
}

#[test]
fn request_child_nodes_delivers_them_as_an_event() {
    open();
    let root = call("DOM.getDocument", json!({}))["root"].clone();
    let html_id = root["children"].as_array().unwrap().iter().find(|n| n["nodeName"] == "HTML").unwrap()["nodeId"].clone();
    let (result, events) = handle("DOM.requestChildNodes", &json!({"nodeId": html_id})).unwrap().unwrap();
    assert_eq!(result, json!({}));
    assert_eq!(events[0].0, "DOM.setChildNodes");
    assert_eq!(events[0].1["parentId"], html_id);
    let names: Vec<&str> = events[0].1["nodes"].as_array().unwrap().iter().map(|n| n["nodeName"].as_str().unwrap()).collect();
    assert_eq!(names, ["HEAD", "BODY"]);
}

/// The Computed pane shows what `getComputedStyle` answers, not a second
/// derivation of it.
#[test]
fn computed_style_answers_what_get_computed_style_answers() {
    let handle_id = open();
    let root = call("DOM.getDocument", json!({"depth": -1}))["root"].clone();
    let p = find(&root, "P").unwrap()["nodeId"].clone();
    let answer = call("CSS.getComputedStyleForNode", json!({"nodeId": p}));
    let value = |name: &str| {
        answer["computedStyle"].as_array().unwrap().iter().find(|e| e["name"] == name).map(|e| e["value"].clone())
    };
    let expected = rts_dom::store::with_dom(handle_id, |dom| {
        let node = dom.query(".card").unwrap();
        ["color", "font-size", "padding-top", "margin-top", "display"].map(|name| dom.computed_property(node, name))
    })
    .unwrap();
    assert_eq!(expected[0], "rgb(255, 0, 0)");
    assert_eq!(expected[1], "20px");
    for (name, want) in ["color", "font-size", "padding-top", "margin-top", "display"].iter().zip(expected) {
        assert_eq!(value(name), Some(Value::String(want)), "{name}");
    }
}

#[test]
fn matched_styles_list_author_rules_with_selector_origin_and_specificity() {
    open();
    let root = call("DOM.getDocument", json!({"depth": -1}))["root"].clone();
    let p = find(&root, "P").unwrap()["nodeId"].clone();
    let answer = call("CSS.getMatchedStylesForNode", json!({"nodeId": p}));
    let rules = answer["matchedCSSRules"].as_array().unwrap();
    let author: Vec<&Value> = rules.iter().filter(|r| r["rule"]["origin"] == "regular").collect();
    assert_eq!(author.len(), 2);
    // Weakest first: `.card` (0,1,0) before `#main .card` (1,1,0).
    assert_eq!(author[0]["rule"]["selectorList"]["text"], ".card");
    assert_eq!(author[0]["rule"]["selectorList"]["selectors"][0]["specificity"], json!({"a": 0, "b": 1, "c": 0}));
    assert_eq!(author[1]["rule"]["selectorList"]["text"], "#main .card");
    let declaration = &author[1]["rule"]["style"]["cssProperties"][0];
    assert_eq!(declaration["name"], "margin-top");
    assert_eq!(declaration["important"], true);
    assert_eq!(answer["inlineStyle"]["cssProperties"][0]["name"], "font-size");
    assert_eq!(answer["inlineStyle"]["cssProperties"][0]["value"], "20px");
}

#[test]
fn highlight_is_the_overlay_state_the_window_reads() {
    let handle_id = open();
    let root = call("DOM.getDocument", json!({"depth": -1}))["root"].clone();
    let p = find(&root, "P").unwrap()["nodeId"].clone();
    call("Overlay.highlightNode", json!({"nodeId": p, "highlightConfig": {}}));
    assert!(rts_dom::overlay::highlight(handle_id).is_some());
    call("Overlay.hideHighlight", json!({}));
    assert!(rts_dom::overlay::highlight(handle_id).is_none());
}

#[test]
fn box_model_nests_content_inside_padding_inside_border() {
    open();
    let root = call("DOM.getDocument", json!({"depth": -1}))["root"].clone();
    let p = find(&root, "P").unwrap()["nodeId"].clone();
    let model = call("DOM.getBoxModel", json!({"nodeId": p}))["model"].clone();
    let x = |quad: &str| model[quad][0].as_f64().unwrap();
    assert_eq!(x("padding") - x("content"), -4.0);
    assert!(x("margin") <= x("border"));
}

#[test]
fn an_unimplemented_method_refuses_by_name() {
    open();
    let refused = handle("DOM.setOuterHTML", &json!({})).unwrap().unwrap_err();
    assert!(refused.contains("DOM.setOuterHTML"));
    assert!(handle("Runtime.evaluate", &json!({})).is_none());
}
