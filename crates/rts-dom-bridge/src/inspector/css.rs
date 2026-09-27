//! The `CSS` domain's reads: computed, matched and inline style.
//!
//! Every value here is the answer a program already gets: the computed value
//! is `Dom::computed_property` — the `getComputedStyle` seam — and the matched
//! rules are `Dom::matched_rules_view`, which runs the cascade's own matcher.
//! Nothing is re-derived for the inspector.

use rts_dom::{Dom, NodeId};
use serde_json::{Value, json};

/// The properties `CSS.getComputedStyleForNode` lists.
///
/// A projection, not a second source of property names: the values come from
/// `computed_property`, and a name it does not know answers `""` and is left
/// out. The list is the set the Computed pane is useful for; Chrome's ~350 are
/// phase 2, once `rts-dom` can enumerate what it models.
const COMPUTED: &[&str] = &[
    "display", "position", "float", "clear", "box-sizing", "width", "height",
    "min-width", "min-height", "max-width", "max-height", "top", "right", "bottom",
    "left", "z-index", "margin-top", "margin-right", "margin-bottom", "margin-left",
    "padding-top", "padding-right", "padding-bottom", "padding-left",
    "border-top-width", "border-right-width", "border-bottom-width",
    "border-left-width", "border-top-style", "border-right-style",
    "border-bottom-style", "border-left-style", "border-top-color",
    "border-right-color", "border-bottom-color", "border-left-color",
    "border-top-left-radius", "border-top-right-radius",
    "border-bottom-right-radius", "border-bottom-left-radius", "color",
    "background-color", "background-image", "opacity", "visibility", "overflow-x",
    "overflow-y", "font-family", "font-size", "font-weight", "font-style",
    "line-height", "letter-spacing", "text-align", "text-decoration-line",
    "text-transform", "white-space", "word-break", "vertical-align", "flex-direction",
    "flex-wrap", "flex-grow", "flex-shrink", "flex-basis", "justify-content",
    "align-items", "align-self", "align-content", "order", "gap", "row-gap",
    "column-gap", "grid-template-columns", "grid-template-rows", "grid-auto-flow",
    "transform", "box-shadow", "cursor", "pointer-events", "list-style-type",
    "writing-mode", "direction",
];

/// `CSS.getComputedStyleForNode`.
pub(super) fn computed(dom: &Dom, node: NodeId) -> Value {
    let properties: Vec<Value> = COMPUTED
        .iter()
        .filter_map(|name| {
            let value = dom.computed_property(node, name);
            (!value.is_empty()).then(|| json!({"name": name, "value": value}))
        })
        .collect();
    json!({"computedStyle": properties})
}

/// A `CSSStyle` of declarations with no stylesheet behind it — read-only in
/// the frontend, which is the truth until `setStyleTexts` (phase 2).
fn style_of(declarations: &[(String, String, bool)]) -> Value {
    let properties: Vec<Value> = declarations
        .iter()
        .map(|(name, value, important)| {
            let text = format!("{name}: {value}{};", if *important { " !important" } else { "" });
            json!({"name": name, "value": value, "important": important, "text": text,
                   "implicit": false, "disabled": false})
        })
        .collect();
    json!({"cssProperties": properties, "shorthandEntries": []})
}

/// `CSS.getMatchedStylesForNode`: the inline style and the matched rules,
/// weakest first, each with its origin and specificity.
///
/// `inherited` is empty: the rules of ancestors whose inherited properties
/// reach this node are phase 2, and the plan names them. The Computed pane is
/// unaffected — it reads the cascade's final answer.
pub(super) fn matched(dom: &Dom, node: NodeId) -> Value {
    let rules: Vec<Value> = dom
        .matched_rules_view(node)
        .into_iter()
        .map(|rule| {
            let (a, b, c) = rule.specificity;
            json!({
                "rule": {
                    "selectorList": {
                        "selectors": [{"text": rule.selector,
                                       "specificity": {"a": a, "b": b, "c": c}}],
                        "text": rule.selector,
                    },
                    "origin": if rule.user_agent { "user-agent" } else { "regular" },
                    "style": style_of(&rule.declarations),
                },
                "matchingSelectors": [0],
            })
        })
        .collect();
    json!({
        "inlineStyle": style_of(&inline_declarations(dom, node)),
        "matchedCSSRules": rules,
        "inherited": [],
        "pseudoElements": [],
    })
}

/// `CSS.getInlineStylesForNode`.
pub(super) fn inline(dom: &Dom, node: NodeId) -> Value {
    json!({"inlineStyle": style_of(&inline_declarations(dom, node))})
}

/// The `style=""` attribute as declarations. A split on `;` outside
/// parentheses and quotes: enough for `url(a;b)` and `content: ";"`.
fn inline_declarations(dom: &Dom, node: NodeId) -> Vec<(String, String, bool)> {
    let text = dom.css_text(node);
    let mut out = Vec::new();
    let (mut depth, mut quote, mut start) = (0i32, None::<char>, 0usize);
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut pieces = Vec::new();
    for &(at, ch) in &bytes {
        match (quote, ch) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(ch),
            (None, '(') => depth += 1,
            (None, ')') => depth -= 1,
            (None, ';') if depth == 0 => {
                pieces.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
    }
    pieces.push(&text[start..]);
    for piece in pieces {
        let Some((name, value)) = piece.split_once(':') else { continue };
        let value = value.trim();
        let (value, important) = match value.strip_suffix("!important") {
            Some(rest) => (rest.trim_end(), true),
            None => (value, false),
        };
        out.push((name.trim().to_ascii_lowercase(), value.to_owned(), important));
    }
    out
}
