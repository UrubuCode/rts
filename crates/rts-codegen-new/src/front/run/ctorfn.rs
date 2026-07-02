//! Lift an ES5-style constructor FUNCTION into a synthetic `class`.
//!
//! A top-level `function F(args) { this.x = …; }` that writes to `this` is the
//! constructor idiom (`new F(args)`). The engine has no `this` binding for a plain
//! function, so such a function OTHERWISE bails at lowering ("property write on a
//! captured `this`") — meaning the whole program already fails to compile. This
//! pre-pass rewrites it into a `class F { x; constructor(args){ this.x = … } }`,
//! reusing the entire class pipeline (shape, ctor with a real `this` param, field
//! slots, `new F()` dispatch). Because the input always failed before, the
//! transform is strictly non-regressing: it only turns a guaranteed bail into a
//! working class.
//!
//! Fields are discovered from the `this.<ident> = …` assignments in the body (in
//! first-seen order). The walk does NOT descend into nested non-arrow functions or
//! classes (their `this` is a different binding); arrows DO share `this`, so a
//! `this.x =` inside a nested arrow is still a field of this constructor.

use std::collections::HashSet;

use rts_ast::ast::{
    ClassDecl, ClassMember, ConstructorDecl, FunctionDecl, Item, MemberModifiers, Program,
    PropertyDecl, Statement,
};
use swc_ecma_visit::{Visit, VisitWith};

/// Rewrite every qualifying top-level constructor-function in `program` into a
/// synthetic `class`. Non-qualifying items pass through unchanged.
pub(super) fn lift_constructor_functions(mut program: Program) -> Program {
    // Functions that are the target of a `new` somewhere in the program (callee
    // unwrapped through `(F)` / `F as any`). A field-LESS `function F(){}` used as
    // `new F()` is still a constructor (an empty instance) and must become a class
    // — otherwise `new F()` bails "not a user class". A function with `this.x=`
    // writes is a constructor regardless of how it is referenced.
    use std::collections::HashMap;
    let newed = collect_new_targets(&program);
    // Each function's OWN `this.<field>` writes (no `.call` expansion) — the lookup
    // a derived constructor uses to absorb a mixed-in base's fields.
    let mut own_fields: HashMap<String, Vec<String>> = HashMap::new();
    for item in &program.items {
        if let Item::Function(f) = item {
            if !f.is_async {
                let fields = discover_this_fields(&f.body, &HashMap::new());
                if !fields.is_empty() {
                    own_fields.insert(f.name.clone(), fields);
                }
            }
        }
    }
    // ES5 prototype methods: `F.prototype.m = __fnprop_N` (the parser hoists the
    // assigned fn-expr to a top-level `__fnprop_N`). Map class → [(method, fn)].
    let mut proto_methods: HashMap<String, Vec<(String, String)>> = HashMap::new();
    // ES5 prototype-chain inheritance: `F.prototype = Object.create(G.prototype)`
    // → the lifted `class F` gets `extends G` (map F → G). The statement itself
    // is consumed (the class model carries the chain).
    let mut extends_map: HashMap<String, String> = HashMap::new();
    for item in &program.items {
        if let Item::Statement(Statement::Raw(raw)) = item {
            if let Some(stmt) = &raw.stmt {
                if let Some((cls, m, fnid)) = parse_proto_method(stmt) {
                    proto_methods.entry(cls).or_default().push((m, fnid));
                }
                if let Some((cls, base)) = parse_proto_extends(stmt) {
                    extends_map.insert(cls, base);
                }
                // Descriptor form: `F.prototype = Object.create(proto, { m:
                // { value: fn, … }, … })` — each `value:` fn becomes a method;
                // a non-`Object` proto base becomes `extends`.
                if let Some((cls, base, methods)) = parse_proto_descriptor(stmt) {
                    if let Some(b) = base {
                        extends_map.insert(cls.clone(), b);
                    }
                    proto_methods.entry(cls).or_default().extend(methods);
                }
            }
        }
    }
    // A name is a constructor iff it has `this.x=` fields, is `new`ed, has a
    // prototype method assigned, or participates in a prototype-chain link
    // (either side of `F.prototype = Object.create(G.prototype)` — the BASE must
    // also lift so `extends` resolves). Only such names trigger conversion /
    // consumption.
    let extends_bases: HashSet<String> = extends_map.values().cloned().collect();
    let is_ctor = |name: &str| {
        own_fields.contains_key(name)
            || newed.contains(name)
            || proto_methods.contains_key(name)
            || extends_map.contains_key(name)
            || extends_bases.contains(name)
    };
    // Hoisted fns moved into a class as methods — dropped from the top level.
    let consumed_fns: HashSet<String> = proto_methods
        .iter()
        .filter(|(cls, _)| is_ctor(cls))
        .flat_map(|(_, ms)| ms.iter().map(|(_, fnid)| fnid.clone()))
        .collect();
    // Bodies of the hoisted method fns, by name.
    let fn_bodies: HashMap<String, FunctionDecl> = program
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Function(f) if consumed_fns.contains(&f.name) => Some((f.name.clone(), f.clone())),
            _ => None,
        })
        .collect();

    let items = std::mem::take(&mut program.items);
    program.items = items
        .into_iter()
        .filter_map(|item| match item {
            // Drop a hoisted method fn (it now lives inside its class).
            Item::Function(ref f) if consumed_fns.contains(&f.name) => None,
            // Drop a consumed `F.prototype.m = …` assignment statement.
            Item::Statement(Statement::Raw(ref raw))
                if raw
                    .stmt
                    .as_ref()
                    .and_then(parse_proto_method)
                    .is_some_and(|(cls, _, _)| is_ctor(&cls)) =>
            {
                None
            }
            // Drop a consumed `F.prototype = Object.create(G.prototype)` link
            // (it became `class F extends G`).
            Item::Statement(Statement::Raw(ref raw))
                if raw
                    .stmt
                    .as_ref()
                    .and_then(parse_proto_extends)
                    .is_some_and(|(cls, _)| is_ctor(&cls)) =>
            {
                None
            }
            // Drop a consumed descriptor-form proto replace (its `value:` fns
            // became methods; a non-Object base became `extends`).
            Item::Statement(Statement::Raw(ref raw))
                if raw
                    .stmt
                    .as_ref()
                    .and_then(parse_proto_descriptor)
                    .is_some_and(|(cls, _, _)| is_ctor(&cls)) =>
            {
                None
            }
            // Drop the `F.prototype.constructor = F` re-link idiom — the class
            // model carries the constructor identity (`inst.constructor.name`
            // resolves from the static class), so the statement is a no-op here.
            Item::Statement(Statement::Raw(ref raw))
                if raw
                    .stmt
                    .as_ref()
                    .and_then(parse_proto_ctor_link)
                    .is_some_and(|cls| is_ctor(&cls)) =>
            {
                None
            }
            // `async function` is its own rewrite (returns a Promise) — never a ctor.
            Item::Function(f) if !f.is_async => {
                let protos = proto_methods.get(&f.name);
                let super_class = extends_map.get(&f.name).cloned();
                Some(
                    match function_to_class(
                        &f,
                        is_ctor(&f.name),
                        &own_fields,
                        protos,
                        &fn_bodies,
                        super_class,
                    ) {
                        Some(class) => Item::Class(class),
                        None => Item::Function(f),
                    },
                )
            }
            other => Some(other),
        })
        .collect();
    program
}

/// Parse `F.prototype.m = <ident>` (the hoisted prototype-method form) →
/// `(class "F", method "m", fn-ident)`. `None` for any other statement — and
/// deliberately `None` for `m == "constructor"` (the ES5 re-link idiom
/// `F.prototype.constructor = F` is NOT a method: treating it as one used to
/// CONSUME the constructor function itself, dropping `F` from the program —
/// "unbound identifier F". See [`parse_proto_ctor_link`]).
fn parse_proto_method(stmt: &swc_ecma_ast::Stmt) -> Option<(String, String, String)> {
    use swc_ecma_ast::{AssignTarget, Expr, MemberProp, SimpleAssignTarget, Stmt};
    let Stmt::Expr(es) = stmt else { return None };
    let Expr::Assign(a) = es.expr.as_ref() else {
        return None;
    };
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left else {
        return None;
    };
    // `m` is `F.prototype.<method>`: prop = method, obj = `F.prototype`.
    let MemberProp::Ident(method) = &m.prop else {
        return None;
    };
    if method.sym.as_ref() == "constructor" {
        return None;
    }
    let Expr::Member(inner) = m.obj.as_ref() else {
        return None;
    };
    let MemberProp::Ident(proto) = &inner.prop else {
        return None;
    };
    if proto.sym.as_ref() != "prototype" {
        return None;
    }
    let cls = callee_ident(&inner.obj)?;
    let fnid = callee_ident(&a.right)?;
    Some((cls, method.sym.to_string(), fnid))
}

/// Parse the ES5 constructor RE-LINK idiom `F.prototype.constructor = F` →
/// `Some("F")`. Only the exact self-link form matches (LHS class == RHS ident);
/// anything else passes through to the normal lowering.
fn parse_proto_ctor_link(stmt: &swc_ecma_ast::Stmt) -> Option<String> {
    use swc_ecma_ast::{AssignTarget, Expr, MemberProp, SimpleAssignTarget, Stmt};
    let Stmt::Expr(es) = stmt else { return None };
    let Expr::Assign(a) = es.expr.as_ref() else {
        return None;
    };
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left else {
        return None;
    };
    let MemberProp::Ident(method) = &m.prop else {
        return None;
    };
    if method.sym.as_ref() != "constructor" {
        return None;
    }
    let Expr::Member(inner) = m.obj.as_ref() else {
        return None;
    };
    let MemberProp::Ident(proto) = &inner.prop else {
        return None;
    };
    if proto.sym.as_ref() != "prototype" {
        return None;
    }
    let cls = callee_ident(&inner.obj)?;
    let rhs = callee_ident(&a.right)?;
    (cls == rhs).then_some(cls)
}

/// Parse the ES5 prototype-chain link `F.prototype = Object.create(G.prototype)`
/// → `Some(("F", "G"))`. The lifted `class F` gets `extends G` (field
/// flattening + method inheritance + `instanceof` chain all ride the class
/// model). `None` for any other statement.
fn parse_proto_extends(stmt: &swc_ecma_ast::Stmt) -> Option<(String, String)> {
    use swc_ecma_ast::{AssignTarget, Callee, Expr, MemberProp, SimpleAssignTarget, Stmt};
    let Stmt::Expr(es) = stmt else { return None };
    let Expr::Assign(a) = es.expr.as_ref() else {
        return None;
    };
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left else {
        return None;
    };
    // LHS must be exactly `F.prototype`.
    let MemberProp::Ident(proto) = &m.prop else {
        return None;
    };
    if proto.sym.as_ref() != "prototype" {
        return None;
    }
    let cls = callee_ident(&m.obj)?;
    // RHS must be `Object.create(G.prototype)`.
    let Expr::Call(call) = a.right.as_ref() else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(cm) = callee.as_ref() else {
        return None;
    };
    let MemberProp::Ident(create) = &cm.prop else {
        return None;
    };
    if create.sym.as_ref() != "create" || callee_ident(&cm.obj).as_deref() != Some("Object") {
        return None;
    }
    let arg = call.args.first()?;
    if arg.spread.is_some() {
        return None;
    }
    let Expr::Member(am) = arg.expr.as_ref() else {
        return None;
    };
    let MemberProp::Ident(aproto) = &am.prop else {
        return None;
    };
    if aproto.sym.as_ref() != "prototype" {
        return None;
    }
    let base = callee_ident(&am.obj)?;
    Some((cls, base))
}

/// Parse the DESCRIPTOR-map proto replace
/// `F.prototype = Object.create(<G|Object>.prototype, { m: { value: fn, … }, … })`
/// → `Some((cls, base, methods))` where `base` is `None` for `Object.prototype`
/// (no user parent) and `methods` maps each descriptor key with a `value:` fn
/// (the parser hoists the fn-expr to a top-level ident) to that fn's name.
/// `None` for any other statement.
fn parse_proto_descriptor(
    stmt: &swc_ecma_ast::Stmt,
) -> Option<(String, Option<String>, Vec<(String, String)>)> {
    use swc_ecma_ast::{
        AssignTarget, Callee, Expr, MemberProp, Prop, PropName, PropOrSpread, SimpleAssignTarget,
        Stmt,
    };
    let Stmt::Expr(es) = stmt else { return None };
    let Expr::Assign(a) = es.expr.as_ref() else {
        return None;
    };
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = &a.left else {
        return None;
    };
    let MemberProp::Ident(proto) = &m.prop else {
        return None;
    };
    if proto.sym.as_ref() != "prototype" {
        return None;
    }
    let cls = callee_ident(&m.obj)?;
    // RHS `Object.create(<base>.prototype, { … })` — exactly 2 args.
    let Expr::Call(call) = a.right.as_ref() else {
        return None;
    };
    let Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let Expr::Member(cm) = callee.as_ref() else {
        return None;
    };
    let MemberProp::Ident(create) = &cm.prop else {
        return None;
    };
    if create.sym.as_ref() != "create"
        || callee_ident(&cm.obj).as_deref() != Some("Object")
        || call.args.len() != 2
        || call.args.iter().any(|a| a.spread.is_some())
    {
        return None;
    }
    // arg0: `<base>.prototype`.
    let Expr::Member(am) = call.args[0].expr.as_ref() else {
        return None;
    };
    let MemberProp::Ident(aproto) = &am.prop else {
        return None;
    };
    if aproto.sym.as_ref() != "prototype" {
        return None;
    }
    let base_name = callee_ident(&am.obj)?;
    let base = (base_name != "Object").then_some(base_name);
    // arg1: `{ m: { value: <fn-ident>, … }, … }` — every prop must contribute a
    // `value:` fn (a getter/setter descriptor is not modeled here → None, so
    // the statement falls through to the normal lowering and bails honestly).
    let Expr::Object(desc_obj) = call.args[1].expr.as_ref() else {
        return None;
    };
    let mut methods: Vec<(String, String)> = Vec::new();
    for p in &desc_obj.props {
        let PropOrSpread::Prop(prop) = p else {
            return None;
        };
        let Prop::KeyValue(kv) = prop.as_ref() else {
            return None;
        };
        let name = match &kv.key {
            PropName::Ident(id) => id.sym.to_string(),
            PropName::Str(s) => s.value.to_string_lossy().into_owned(),
            _ => return None,
        };
        // The descriptor body: an object literal with a `value:` key whose value
        // is a (hoisted) fn ident. `writable`/`configurable`/`enumerable` flags
        // are irrelevant to the class model and ignored.
        let Expr::Object(desc) = kv.value.as_ref() else {
            return None;
        };
        let mut fnid: Option<String> = None;
        for dp in &desc.props {
            let PropOrSpread::Prop(dprop) = dp else {
                return None;
            };
            let Prop::KeyValue(dkv) = dprop.as_ref() else {
                return None;
            };
            let dname = match &dkv.key {
                PropName::Ident(id) => id.sym.to_string(),
                PropName::Str(s) => s.value.to_string_lossy().into_owned(),
                _ => return None,
            };
            if dname == "value" {
                fnid = callee_ident(&dkv.value);
            }
            if dname == "get" || dname == "set" {
                // Accessor descriptors are a different surface — not lifted.
                return None;
            }
        }
        methods.push((name, fnid?));
    }
    Some((cls, base, methods))
}

/// Build a synthetic `ClassDecl` from `f` iff it is a constructor: it writes to
/// `this.<field>`, OR it is used as a `new` target (`is_newed`). `None` for an
/// ordinary function — left untouched. `own_fields` maps each constructor name to
/// its direct `this.<field>` writes, so a `Base.call(this, …)` mixin contributes
/// Base's fields to this constructor's shape.
fn function_to_class(
    f: &FunctionDecl,
    is_ctor: bool,
    own_fields: &std::collections::HashMap<String, Vec<String>>,
    proto_methods: Option<&Vec<(String, String)>>,
    fn_bodies: &std::collections::HashMap<String, FunctionDecl>,
    super_class: Option<String>,
) -> Option<ClassDecl> {
    let fields = discover_this_fields(&f.body, own_fields);
    if fields.is_empty() && !is_ctor {
        return None;
    }
    let mut members: Vec<ClassMember> = fields
        .into_iter()
        .map(|name| {
            ClassMember::Property(PropertyDecl {
                name,
                modifiers: MemberModifiers::default(),
                type_annotation: None,
                initializer: None,
                span: f.span,
            })
        })
        .collect();
    members.push(ClassMember::Constructor(ConstructorDecl {
        parameters: f.parameters.clone(),
        body: f.body.clone(),
        span: f.span,
    }));
    // ES5 prototype methods: each `F.prototype.m = __fnprop_N` becomes a real
    // method `m` (the hoisted fn's params/body, `this` now bound by the class).
    if let Some(protos) = proto_methods {
        for (method, fnid) in protos {
            if let Some(mf) = fn_bodies.get(fnid) {
                members.push(ClassMember::Method(rts_ast::ast::MethodDecl {
                    name: method.clone(),
                    modifiers: MemberModifiers::default(),
                    parameters: mf.parameters.clone(),
                    return_type: mf.return_type.clone(),
                    body: mf.body.clone(),
                    role: rts_ast::ast::MethodRole::Method,
                    span: mf.span,
                }));
            }
        }
    }
    Some(ClassDecl {
        name: f.name.clone(),
        super_class,
        members,
        is_abstract: false,
        static_init_body: Vec::new(),
        static_init_blocks: Vec::new(),
        exported: f.exported,
        span: f.span,
    })
}

/// Collect the names that appear as `new <name>(…)` anywhere in the program
/// (the callee unwrapped through `(…)` / `… as T` casts). Walks every top-level
/// statement and function/class-method body.
fn collect_new_targets(program: &Program) -> HashSet<String> {
    let mut c = NewTargetCollector { out: HashSet::new() };
    for item in &program.items {
        match item {
            Item::Statement(Statement::Raw(raw)) => {
                if let Some(stmt) = &raw.stmt {
                    stmt.visit_with(&mut c);
                }
            }
            Item::Function(f) => {
                for Statement::Raw(raw) in &f.body {
                    if let Some(stmt) = &raw.stmt {
                        stmt.visit_with(&mut c);
                    }
                }
            }
            Item::Class(cl) => {
                for m in &cl.members {
                    let body = match m {
                        rts_ast::ast::ClassMember::Constructor(c) => &c.body,
                        rts_ast::ast::ClassMember::Method(m) => &m.body,
                        rts_ast::ast::ClassMember::Property(_) => continue,
                    };
                    for Statement::Raw(raw) in body {
                        if let Some(stmt) = &raw.stmt {
                            stmt.visit_with(&mut c);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    c.out
}

struct NewTargetCollector {
    out: HashSet<String>,
}

impl Visit for NewTargetCollector {
    fn visit_new_expr(&mut self, node: &swc_ecma_ast::NewExpr) {
        if let Some(name) = callee_ident(&node.callee) {
            self.out.insert(name);
        }
        node.visit_children_with(self);
    }
}

/// The base identifier of a `new`/call callee, unwrapping `(…)` / `… as T` /
/// `<T>…` / `…!` casts (`new (F as any)()` → `F`).
fn callee_ident(e: &swc_ecma_ast::Expr) -> Option<String> {
    use swc_ecma_ast::Expr;
    match e {
        Expr::Ident(id) => Some(id.sym.to_string()),
        Expr::Paren(p) => callee_ident(&p.expr),
        Expr::TsAs(a) => callee_ident(&a.expr),
        Expr::TsConstAssertion(a) => callee_ident(&a.expr),
        Expr::TsNonNull(a) => callee_ident(&a.expr),
        Expr::TsTypeAssertion(a) => callee_ident(&a.expr),
        _ => None,
    }
}

/// Collect the field names of a constructor `body`, first-seen order: each
/// `this.<ident> = …` write, plus the fields of any `Base.call(this, …)` mixin
/// (looked up in `own_fields`, one level — enough for `Derived → Base` chains).
fn discover_this_fields(
    body: &[Statement],
    own_fields: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut c = ThisFieldCollector {
        out: Vec::new(),
        seen: HashSet::new(),
        own_fields,
    };
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = &raw.stmt {
            stmt.visit_with(&mut c);
        }
    }
    c.out
}

struct ThisFieldCollector<'a> {
    out: Vec<String>,
    seen: HashSet<String>,
    own_fields: &'a std::collections::HashMap<String, Vec<String>>,
}

impl<'a> ThisFieldCollector<'a> {
    fn add(&mut self, name: String) {
        if self.seen.insert(name.clone()) {
            self.out.push(name);
        }
    }
}

impl<'a> Visit for ThisFieldCollector<'a> {
    fn visit_assign_expr(&mut self, node: &swc_ecma_ast::AssignExpr) {
        if let Some(name) = this_member_field(&node.left) {
            self.add(name);
        }
        node.visit_children_with(self);
    }

    fn visit_call_expr(&mut self, node: &swc_ecma_ast::CallExpr) {
        // `Base.call(this, …)` mixin: absorb Base's own fields at this position so
        // the derived shape has slots for what Base's body will write.
        if let swc_ecma_ast::Callee::Expr(callee) = &node.callee {
            if let swc_ecma_ast::Expr::Member(m) = callee.as_ref() {
                if let swc_ecma_ast::MemberProp::Ident(mp) = &m.prop {
                    if mp.sym.as_ref() == "call" {
                        // first arg is `this` → this is a constructor mixin
                        let first_is_this = node
                            .args
                            .first()
                            .is_some_and(|a| a.spread.is_none() && is_this_expr(&a.expr));
                        if first_is_this {
                            if let Some(base) = callee_ident(&m.obj) {
                                if let Some(bf) = self.own_fields.get(&base) {
                                    for name in bf.clone() {
                                        self.add(name);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        node.visit_children_with(self);
    }

    // A nested non-arrow function / class has its OWN `this` — do not descend.
    fn visit_function(&mut self, _node: &swc_ecma_ast::Function) {}
    fn visit_class(&mut self, _node: &swc_ecma_ast::Class) {}
}

/// `Some("x")` iff `target` is exactly `this.x` / `(this as any).x` (a simple
/// member of `this`, with `this` possibly wrapped in `as`/parens, and an
/// identifier key); `None` otherwise (`this[expr]`, `a.x`, a bare ident, …).
fn this_member_field(target: &swc_ecma_ast::AssignTarget) -> Option<String> {
    use swc_ecma_ast::{AssignTarget, MemberProp, SimpleAssignTarget};
    let AssignTarget::Simple(SimpleAssignTarget::Member(m)) = target else {
        return None;
    };
    if !is_this_expr(&m.obj) {
        return None;
    }
    match &m.prop {
        MemberProp::Ident(id) => Some(id.sym.to_string()),
        _ => None,
    }
}

/// Whether `e` is `this`, possibly wrapped in `(…)` / `… as T` / `<T>…` casts
/// (`(this as any)`, `(this)`, `this!`) — all of which preserve the `this` value.
fn is_this_expr(e: &swc_ecma_ast::Expr) -> bool {
    use swc_ecma_ast::Expr;
    match e {
        Expr::This(_) => true,
        Expr::Paren(p) => is_this_expr(&p.expr),
        Expr::TsAs(a) => is_this_expr(&a.expr),
        Expr::TsConstAssertion(a) => is_this_expr(&a.expr),
        Expr::TsNonNull(a) => is_this_expr(&a.expr),
        Expr::TsTypeAssertion(a) => is_this_expr(&a.expr),
        _ => false,
    }
}
