//! CDP node ids, and the `DOM` domain's reads.
//!
//! # The side table, and why it is not a root list
//!
//! CDP names a node by a small integer that must stay the same for the life of
//! the document. `NodeId` is `(generation, idx)`, which already has that
//! stability, but is 64 bits and packs two numbers — so the frontend gets a
//! counter, and this table maps it back. It holds `(document handle, NodeId)`:
//! plain data in `rts-dom`'s own arena, never a handle into the runtime's
//! region, so `rts-core` rule 10 (a table of runtime handles is a root list)
//! does not apply. A stale entry fails `Dom::resolve` by generation and reads
//! as "no such node". The table grows only as nodes are SENT, and only while
//! an inspector is attached.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use rts_dom::{Dom, NodeId, NodeKind};
use serde_json::{Value, json};

#[derive(Default)]
struct Table {
    by_node: HashMap<(u64, i64), u32>,
    nodes: Vec<(u64, NodeId)>,
    /// Nodes whose children the frontend already holds.
    expanded: HashSet<u32>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

/// The CDP id of `node`, assigning one on first sight. Ids start at 1: CDP
/// reads 0 as "no node".
pub(super) fn id_of(handle: u64, node: NodeId) -> u32 {
    TABLE.with(|table| {
        let mut table = table.borrow_mut();
        let key = (handle, node.to_abi());
        if let Some(&id) = table.by_node.get(&key) {
            return id;
        }
        table.nodes.push((handle, node));
        let id = table.nodes.len() as u32;
        table.by_node.insert(key, id);
        id
    })
}

/// The node behind a CDP id.
pub(super) fn node_of(id: u32) -> Option<(u64, NodeId)> {
    TABLE.with(|table| table.borrow().nodes.get((id as usize).checked_sub(1)?).copied())
}

/// The node a request names by `nodeId` or `backendNodeId` — they are one
/// number here.
pub(super) fn node_param(params: &Value) -> Result<(u64, NodeId), String> {
    let id = params
        .get("nodeId")
        .or_else(|| params.get("backendNodeId"))
        .and_then(Value::as_u64)
        .ok_or("a nodeId or backendNodeId is required")?;
    node_of(id as u32).ok_or_else(|| format!("no node with id {id}"))
}

fn mark_expanded(id: u32) {
    TABLE.with(|table| table.borrow_mut().expanded.insert(id));
}

fn is_expanded(id: u32) -> bool {
    TABLE.with(|table| table.borrow().expanded.contains(&id))
}

/// The children the Elements panel shows: whitespace-only text is left out, as
/// Chrome leaves it out.
fn shown_children(dom: &Dom, node: NodeId) -> Vec<NodeId> {
    dom.child_nodes(node)
        .into_iter()
        .filter(|child| {
            dom.resolve(*child).is_some_and(|idx| match &dom.nodes[idx].kind {
                NodeKind::Text(text) => !text.trim().is_empty(),
                _ => true,
            })
        })
        .collect()
}

/// One node as CDP's `DOM.Node`, with its children down to `depth` (`-1` is
/// the whole subtree).
pub(super) fn describe(dom: &Dom, handle: u64, node: NodeId, depth: i64) -> Value {
    let id = id_of(handle, node);
    let Some(idx) = dom.resolve(node) else {
        return json!({"nodeId": id, "backendNodeId": id, "nodeType": 1, "nodeName": "", "localName": "", "nodeValue": ""});
    };
    let children = shown_children(dom, node);
    let raw = &dom.nodes[idx];
    let (node_type, name, local, value) = match &raw.kind {
        NodeKind::Document => (9, "#document".to_owned(), String::new(), String::new()),
        NodeKind::Element { tag } => (1, tag.to_ascii_uppercase(), tag.clone(), String::new()),
        NodeKind::Text(text) => (3, "#text".to_owned(), String::new(), text.clone()),
        NodeKind::Comment(text) => (8, "#comment".to_owned(), String::new(), text.clone()),
    };
    let mut out = json!({
        "nodeId": id, "backendNodeId": id, "nodeType": node_type, "nodeName": name,
        "localName": local, "nodeValue": value, "childNodeCount": children.len(),
    });
    if node_type == 1 {
        let attributes: Vec<Value> = raw
            .attrs
            .iter()
            .flat_map(|a| [Value::String(a.name.clone()), Value::String(a.value.clone())])
            .collect();
        out["attributes"] = Value::Array(attributes);
    }
    if node_type == 9 {
        out["documentURL"] = json!(format!("rts://document/{handle}"));
        out["baseURL"] = out["documentURL"].clone();
        out["xmlVersion"] = json!("");
    }
    if depth != 0 {
        mark_expanded(id);
        let list: Vec<Value> = children
            .into_iter()
            .map(|child| describe(dom, handle, child, depth - 1))
            .collect();
        out["children"] = Value::Array(list);
    }
    out
}

/// `DOM.getDocument` — the root, `depth` levels deep (default 1). The
/// frontend drops its whole model when it asks, so the expanded set does too.
pub(super) fn get_document(dom: &Dom, handle: u64, params: &Value) -> Value {
    TABLE.with(|table| table.borrow_mut().expanded.clear());
    let depth = params.get("depth").and_then(Value::as_i64).unwrap_or(1);
    let root = dom.id_of_idx(dom.root);
    json!({"root": describe(dom, handle, root, depth)})
}

/// `DOM.setChildNodes` for `node`: the event that delivers its children.
pub(super) fn set_child_nodes(dom: &Dom, handle: u64, node: NodeId, depth: i64) -> (String, Value) {
    let parent = id_of(handle, node);
    mark_expanded(parent);
    let nodes: Vec<Value> = shown_children(dom, node)
        .into_iter()
        .map(|child| describe(dom, handle, child, depth - 1))
        .collect();
    ("DOM.setChildNodes".to_owned(), json!({"parentId": parent, "nodes": nodes}))
}

/// The events that make `node` known to the frontend: the children of every
/// ancestor the frontend has not expanded yet, from the root down. Without
/// them a `nodeId` answered by `getNodeForLocation` names a node the Elements
/// panel cannot place.
pub(super) fn reveal(dom: &Dom, handle: u64, node: NodeId) -> Vec<(String, Value)> {
    let mut chain = Vec::new();
    let mut at = dom.parent_of(node);
    while let Some(parent) = at {
        chain.push(parent);
        at = dom.parent_of(parent);
    }
    chain
        .into_iter()
        .rev()
        .filter(|parent| !is_expanded(id_of(handle, *parent)))
        .map(|parent| set_child_nodes(dom, handle, parent, 1))
        .collect()
}
