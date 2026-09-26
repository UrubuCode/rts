//! The document half of the inspector: the `DOM`, `CSS`, `Overlay` and `Page`
//! domains of the Chrome DevTools Protocol, over the document `rts:dom` holds.
//!
//! Plan: `docs/superpowers/plans/2026-09-26-dom-inspector.md`. The transport is
//! `rts-node`'s (`inspector::cdp`), and `rts-host` wires [`handle`] into it:
//! this crate does not know a socket, and that one does not know a document.
//! Everything here runs on the program's thread, which is the thread that owns
//! `rts_dom::store`.

mod css;
mod nodes;

use rts_dom::{Dom, NodeId};
use serde_json::{Value, json};

/// Result and events of one method — `rts-node`'s `Reply`, spelled as a tuple
/// so this crate does not depend on that one.
pub type Answer = (Value, Vec<(String, Value)>);

/// Runs `method` when it belongs to a document domain; `None` otherwise.
pub fn handle(method: &str, params: &Value) -> Option<Result<Answer, String>> {
    let (domain, _) = method.split_once('.')?;
    if !matches!(domain, "DOM" | "CSS" | "Overlay" | "Page") {
        return None;
    }
    Some(run(method, params))
}

fn plain(result: Value) -> Result<Answer, String> {
    Ok((result, Vec::new()))
}

/// The document shown: the newest live one (plan F6).
fn document() -> Result<u64, String> {
    rts_dom::store::latest().ok_or_else(|| "there is no document to inspect".to_owned())
}

fn with_document<R>(handle: u64, f: impl FnOnce(&Dom) -> R) -> Result<R, String> {
    rts_dom::store::with_dom(handle, f).ok_or_else(|| "the document is gone".to_owned())
}

fn run(method: &str, params: &Value) -> Result<Answer, String> {
    match method {
        // Nothing to switch: the domains answer whether or not they were
        // enabled, and none of them streams events yet.
        "DOM.enable" | "DOM.disable" | "CSS.enable" | "CSS.disable" | "Overlay.enable"
        | "Overlay.disable" | "Page.disable" => plain(json!({})),

        "Page.getResourceTree" => {
            let handle = document()?;
            let url = format!("rts://document/{handle}");
            plain(json!({"frameTree": {"frame": {
                "id": "rts-frame", "loaderId": "rts", "url": url, "domainAndRegistry": "",
                "securityOrigin": "rts://", "mimeType": "text/html",
                "secureContextType": "Secure", "crossOriginIsolatedContextType": "NotIsolated",
                "gatedAPIFeatures": []
            }, "resources": []}}))
        }

        "DOM.getDocument" => {
            let handle = document()?;
            plain(with_document(handle, |dom| nodes::get_document(dom, handle, params))?)
        }
        "DOM.requestChildNodes" => {
            let (handle, node) = nodes::node_param(params)?;
            let depth = params.get("depth").and_then(Value::as_i64).unwrap_or(1);
            let event = with_document(handle, |dom| nodes::set_child_nodes(dom, handle, node, depth))?;
            Ok((json!({}), vec![event]))
        }
        "DOM.describeNode" => {
            let (handle, node) = nodes::node_param(params)?;
            let depth = params.get("depth").and_then(Value::as_i64).unwrap_or(0);
            plain(json!({"node": with_document(handle, |dom| nodes::describe(dom, handle, node, depth))?}))
        }
        "DOM.pushNodesByBackendIdsToFrontend" => {
            let ids: Vec<u64> = params
                .get("backendNodeIds")
                .and_then(Value::as_array)
                .map(|all| all.iter().filter_map(Value::as_u64).collect())
                .unwrap_or_default();
            let mut events = Vec::new();
            for id in &ids {
                if let Some((handle, node)) = nodes::node_of(*id as u32) {
                    events.extend(with_document(handle, |dom| nodes::reveal(dom, handle, node))?);
                }
            }
            Ok((json!({"nodeIds": ids}), events))
        }
        "DOM.getNodeForLocation" => {
            let handle = document()?;
            let x = params.get("x").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            let y = params.get("y").and_then(Value::as_f64).unwrap_or(0.0) as f32;
            with_document(handle, |dom| {
                let node = dom.node_at(x, y).ok_or("no node at that location")?;
                let id = nodes::id_of(handle, node);
                let events = nodes::reveal(dom, handle, node);
                Ok((json!({"nodeId": id, "backendNodeId": id, "frameId": "rts-frame"}), events))
            })?
        }
        "DOM.getBoxModel" => {
            let (handle, node) = nodes::node_param(params)?;
            with_document(handle, |dom| box_model(dom, node))?.map(|model| (json!({"model": model}), Vec::new()))
        }

        "CSS.getComputedStyleForNode" => {
            let (handle, node) = nodes::node_param(params)?;
            plain(with_document(handle, |dom| css::computed(dom, node))?)
        }
        "CSS.getMatchedStylesForNode" => {
            let (handle, node) = nodes::node_param(params)?;
            plain(with_document(handle, |dom| css::matched(dom, node))?)
        }
        "CSS.getInlineStylesForNode" => {
            let (handle, node) = nodes::node_param(params)?;
            plain(with_document(handle, |dom| css::inline(dom, node))?)
        }

        "Overlay.highlightNode" => {
            let (handle, node) = nodes::node_param(params)?;
            rts_dom::overlay::set_highlight(handle, Some(node));
            plain(json!({}))
        }
        "Overlay.hideHighlight" => {
            if let Some(handle) = rts_dom::store::latest() {
                rts_dom::overlay::set_highlight(handle, None);
            }
            plain(json!({}))
        }

        // Named: the write set, the picker, and everything else of these
        // domains are phase 2 of the plan. A refusal, never an empty answer.
        _ => Err(format!("'{method}' is not implemented by this inspector")),
    }
}

/// `DOM.getBoxModel`: the four boxes as quads, from the border box the layout
/// answers and the used widths `getComputedStyle` answers.
fn box_model(dom: &Dom, node: NodeId) -> Result<Value, String> {
    let [x, y, w, h] = dom.border_box(node).ok_or("the node has no box")?;
    let px = |name: &str| -> f32 {
        dom.computed_property(node, name).trim_end_matches("px").parse().unwrap_or(0.0)
    };
    let sides = |prefix: &str, suffix: &str| {
        ["top", "right", "bottom", "left"].map(|side| px(&format!("{prefix}-{side}{suffix}")))
    };
    let border = sides("border", "-width");
    let padding = sides("padding", "");
    let margin = sides("margin", "");
    let quad = |x0: f32, y0: f32, x1: f32, y1: f32| json!([x0, y0, x1, y0, x1, y1, x0, y1]);
    let (bx0, by0, bx1, by1) = (x, y, x + w, y + h);
    let (px0, py0, px1, py1) = (bx0 + border[3], by0 + border[0], bx1 - border[1], by1 - border[2]);
    let (cx0, cy0, cx1, cy1) = (px0 + padding[3], py0 + padding[0], px1 - padding[1], py1 - padding[2]);
    let (mx0, my0, mx1, my1) = (bx0 - margin[3], by0 - margin[0], bx1 + margin[1], by1 + margin[2]);
    Ok(json!({
        "content": quad(cx0, cy0, cx1, cy1),
        "padding": quad(px0, py0, px1, py1),
        "border": quad(bx0, by0, bx1, by1),
        "margin": quad(mx0, my0, mx1, my1),
        "width": w, "height": h,
    }))
}

#[cfg(test)]
mod tests;
