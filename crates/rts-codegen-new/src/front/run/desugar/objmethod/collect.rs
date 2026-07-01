//! swc collection for the object-literal method recovery: the per-unit pre-order
//! `ObjectLit` collector + the per-literal method extractor.

use swc_ecma_visit::{Visit, VisitWith};

/// One recovered PLAIN method of an object literal: its property name + the swc
/// `Function` (its body rts-hir dropped).
pub(super) struct RecoveredMethod<'a> {
    pub name: String,
    pub function: &'a swc_ecma_ast::Function,
}

/// Collect every `ObjectLit` reachable from the unit's statements in document
/// (pre-order) order. `Tpl` is treated as a LEAF (not descended) because at this
/// stage an object inside a template interpolation is still a `Raw` placeholder in
/// the HIR — descending would desync the positional pairing.
pub(super) fn collect_object_lits(stmts: &[&swc_ecma_ast::Stmt]) -> Vec<swc_ecma_ast::ObjectLit> {
    let mut c = ObjCollector { out: Vec::new() };
    for s in stmts {
        s.visit_with(&mut c);
    }
    c.out
}

struct ObjCollector {
    out: Vec<swc_ecma_ast::ObjectLit>,
}

impl Visit for ObjCollector {
    fn visit_object_lit(&mut self, node: &swc_ecma_ast::ObjectLit) {
        // Record this object PRE-ORDER (before its children), matching the HIR
        // rewrite's pre-order, then descend so nested object literals are recorded
        // in the same order rts-hir lowered them.
        self.out.push(node.clone());
        node.visit_children_with(self);
    }

    fn visit_tpl(&mut self, _node: &swc_ecma_ast::Tpl) {
        // Leaf: an object inside `${ … }` is still a Raw placeholder in HIR here.
    }

    fn visit_tagged_tpl(&mut self, _node: &swc_ecma_ast::TaggedTpl) {
        // Leaf, as for `Tpl`.
    }
}

/// The outcome of inspecting an object literal's non-field props.
pub(super) enum Recovered<'a> {
    /// A plain fieldful literal with NO methods at all — no class needed, no bail.
    Plain,
    /// A literal whose every non-field prop is a recoverable plain method.
    Methods(Vec<RecoveredMethod<'a>>),
    /// A literal carrying a member the engine cannot model (getter/setter/computed/
    /// generator/async method, spread, or non-simple param). The literal MUST bail
    /// at use rather than silently degrade to a partial object (a getter read would
    /// otherwise return `undefined` — a wrong value, not a bail).
    Unsupported,
}

/// Inspect the non-field props of `obj` (see [`Recovered`]). rts-hir already kept
/// the `KeyValue`/`Shorthand` fields; this only classifies the dropped props.
pub(super) fn recover_methods(obj: &swc_ecma_ast::ObjectLit) -> Recovered<'_> {
    use swc_ecma_ast::{Prop, PropOrSpread};

    let mut methods = Vec::new();
    for p in &obj.props {
        match p {
            // A spread `{ ...src }`: rts-hir keeps it as a `"\0spread_<i>"` field
            // (applied at runtime via `obj_assign`) — nothing to recover here.
            PropOrSpread::Spread(_) => {}
            PropOrSpread::Prop(prop) => match prop.as_ref() {
                // Fields: rts-hir already kept these — nothing to recover.
                Prop::KeyValue(_) | Prop::Shorthand(_) => {}
                Prop::Method(m) => {
                    // Plain method only: a generator/async method, a non-identifier
                    // (computed/string/number) name, or a non-simple param → bail.
                    if m.function.is_generator || m.function.is_async {
                        return Recovered::Unsupported;
                    }
                    let Some(name) = prop_ident_name(&m.key) else {
                        return Recovered::Unsupported;
                    };
                    for param in &m.function.params {
                        if super::super::super::class::ident_param_name(&param.pat).is_none() {
                            return Recovered::Unsupported;
                        }
                    }
                    methods.push(RecoveredMethod {
                        name,
                        function: &m.function,
                    });
                }
                // Getter / setter / assign-shorthand → the literal must bail.
                Prop::Getter(_) | Prop::Setter(_) | Prop::Assign(_) => {
                    return Recovered::Unsupported;
                }
                // Defensive: any future variant bails rather than silently drop.
                #[allow(unreachable_patterns)]
                _ => return Recovered::Unsupported,
            },
        }
    }
    if methods.is_empty() {
        Recovered::Plain
    } else {
        Recovered::Methods(methods)
    }
}

/// The identifier name of a non-computed property key, or `None` for a
/// computed / string / numeric / bigint key (out of subset).
fn prop_ident_name(key: &swc_ecma_ast::PropName) -> Option<String> {
    match key {
        swc_ecma_ast::PropName::Ident(id) => Some(id.sym.to_string()),
        _ => None,
    }
}
