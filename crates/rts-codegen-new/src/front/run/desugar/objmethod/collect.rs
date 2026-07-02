//! swc collection for the object-literal method recovery: the per-unit pre-order
//! `ObjectLit` collector + the per-literal method extractor.

use swc_ecma_visit::{Visit, VisitWith};

/// One recovered method-like member of an object literal: its property name +
/// the swc node (its body rts-hir dropped) — a plain METHOD, a GETTER
/// (`get x() {…}`) or a SETTER (`set x(v) {…}`).
pub(super) struct RecoveredMethod<'a> {
    pub name: String,
    pub fn_ref: LitFnRef<'a>,
}

/// The swc node backing a recovered literal member (methods carry a full
/// `Function`; accessors carry their dedicated prop nodes).
pub(super) enum LitFnRef<'a> {
    Method(&'a swc_ecma_ast::Function),
    Getter(&'a swc_ecma_ast::GetterProp),
    Setter(&'a swc_ecma_ast::SetterProp),
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
                    // EXCEPTION: a computed key that IS a well-known `Symbol.X`
                    // member (`[Symbol.toPrimitive](hint) {…}`) recovers as the
                    // internal `@@X` method name — Symbol is PRIMORDIAL, and the
                    // coercion trampolines look the method up by that own-prop
                    // key (`@@` is not spellable as a plain JS identifier key).
                    if m.function.is_generator || m.function.is_async {
                        return Recovered::Unsupported;
                    }
                    let name = match prop_ident_name(&m.key)
                        .or_else(|| wellknown_symbol_method_name(&m.key))
                    {
                        Some(name) => name,
                        None => return Recovered::Unsupported,
                    };
                    for param in &m.function.params {
                        if super::super::super::class::ident_param_name(&param.pat).is_none() {
                            return Recovered::Unsupported;
                        }
                    }
                    methods.push(RecoveredMethod {
                        name,
                        fn_ref: LitFnRef::Method(&m.function),
                    });
                }
                // Accessors — recovered as getter/setter members of the literal
                // class (the same accessor dispatch user classes use).
                Prop::Getter(g) => {
                    let Some(name) = prop_ident_name(&g.key) else {
                        return Recovered::Unsupported;
                    };
                    if g.body.is_none() {
                        return Recovered::Unsupported;
                    }
                    methods.push(RecoveredMethod {
                        name,
                        fn_ref: LitFnRef::Getter(g),
                    });
                }
                Prop::Setter(s) => {
                    let Some(name) = prop_ident_name(&s.key) else {
                        return Recovered::Unsupported;
                    };
                    if s.body.is_none()
                        || super::super::super::class::ident_param_name(&s.param).is_none()
                    {
                        return Recovered::Unsupported;
                    }
                    methods.push(RecoveredMethod {
                        name,
                        fn_ref: LitFnRef::Setter(s),
                    });
                }
                // Assign-shorthand → the literal must bail.
                Prop::Assign(_) => {
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

/// `[Symbol.X](){}` — a COMPUTED key that is literally the well-known `Symbol.X`
/// member expression, recovered as the internal `@@X` method name (`Symbol` is
/// PRIMORDIAL — the engine may name it). The literal class materializes the
/// method as an own-prop fn slot under that key, and the generic coercion
/// trampolines (`to_number`/`to_string`/`__rtsadp_add`) look `@@toPrimitive` up
/// by it. Any other computed key stays out of subset.
fn wellknown_symbol_method_name(key: &swc_ecma_ast::PropName) -> Option<String> {
    let swc_ecma_ast::PropName::Computed(c) = key else {
        return None;
    };
    let swc_ecma_ast::Expr::Member(m) = &*c.expr else {
        return None;
    };
    let swc_ecma_ast::Expr::Ident(obj) = &*m.obj else {
        return None;
    };
    if obj.sym.as_ref() != "Symbol" {
        return None;
    }
    let swc_ecma_ast::MemberProp::Ident(p) = &m.prop else {
        return None;
    };
    Some(format!("@@{}", p.sym))
}
