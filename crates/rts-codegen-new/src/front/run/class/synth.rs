//! Synthesize a class's [`ClassDesc`] + its constructor/method/accessor/static
//! `HirFunc`s from the AST `ClassDecl` (P4.9 single classes, P5.1 inheritance /
//! accessors / statics).
//!
//! The constructor and every method/accessor become ordinary top-level
//! `HirFunc`s whose FIRST parameter is the implicit receiver `this`; a static
//! method becomes a top-level fn with NO `this`. After lowering each body we
//! rewrite swc's `Raw("This(…)")` to `Ident("this")` and the two `super` shapes
//! to direct parent calls ([`super::inherit`]).
//!
//! Field ORDER (flattened): the parent's flattened fields first, then this class's
//! own declared props ∪ first-assigned `this.x` in the constructor, in first-seen
//! order. The global shape-id is interned from that flattened list so every
//! instance shares one flat shape (no prototype walk for fields).

use std::collections::HashMap;

use rts_ast::ast::{ClassDecl, ClassMember, ConstructorDecl, MethodDecl, MethodRole, PropertyDecl};
use rts_hir::ir::{HirExpr, HirExprKind, HirFunc, HirParam, HirStmt, HirType};
use rts_hir::scope::Scope;

use crate::front::error::{FrontResult, Unsupported};

use super::inherit::rewrite_super_block;
use super::walk::{
    body_uses_this, collect_this_assign_fields, push_unique, rewrite_this_block, this_field_assign,
};
use super::{this_param, Accessor, ClassDesc};

/// The synthesized constructor function name for class `name`.
pub(crate) fn ctor_name(name: &str) -> String {
    format!("__rtsn_ctor_{name}")
}

/// The synthesized method function name for `class.method`.
pub(crate) fn method_name(class: &str, method: &str) -> String {
    format!("__rtsn_method_{class}_{method}")
}

/// The synthesized method function name for an OBJECT-LITERAL method `class.method`
/// (P5.15). A distinct prefix from [`method_name`] keeps literal-class methods from
/// ever colliding with a user class's synthesized methods.
pub(crate) fn method_name_lit(class: &str, method: &str) -> String {
    format!("__rtsl_method_{class}_{method}")
}

/// The synthesized getter / setter fn name for `class.prop`.
fn getter_name(class: &str, prop: &str) -> String {
    format!("__rtsn_get_{class}_{prop}")
}
fn setter_name(class: &str, prop: &str) -> String {
    format!("__rtsn_set_{class}_{prop}")
}

/// The synthesized static-method fn name for `class.method`.
pub(crate) fn static_method_name(class: &str, method: &str) -> String {
    format!("__rtsn_static_{class}_{method}")
}

/// The synthesized static-field-getter fn name for `class.field`.
pub(crate) fn static_field_getter_name(class: &str, field: &str) -> String {
    format!("__rtsn_sfield_{class}_{field}")
}

/// Categorized members of one class decl.
struct Members<'a> {
    ctor: Option<&'a ConstructorDecl>,
    methods: Vec<&'a MethodDecl>,
    getters: Vec<&'a MethodDecl>,
    setters: Vec<&'a MethodDecl>,
    static_methods: Vec<&'a MethodDecl>,
    props: Vec<&'a PropertyDecl>,
    static_props: Vec<&'a PropertyDecl>,
}

/// Scan the synthesized constructor in `funcs` (the only `__rtsn_ctor_*` fn) for
/// top-level `this.<field> = [...]` assignments whose RHS is an array literal, and
/// record those field names as array-typed. Only the ctor body's own statements
/// are inspected (initializer prologue assignments are already covered by the
/// PropertyDecl pass — this catches `this.x = []` written explicitly in the ctor).
fn collect_ctor_array_fields(
    funcs: &[HirFunc],
    field_arrays: &mut std::collections::HashSet<String>,
) {
    use super::THIS;
    let Some(ctor) = funcs.iter().find(|f| f.name.starts_with("__rtsn_ctor_")) else {
        return;
    };
    for stmt in &ctor.body {
        if let HirStmt::Expr(e) = stmt {
            if let HirExprKind::Assign { target, value } = &e.kind {
                if let HirExprKind::Member { object, prop } = &target.kind {
                    if matches!(&object.kind, HirExprKind::Ident(n) if n == THIS)
                        && matches!(value.kind, HirExprKind::Array(_))
                    {
                        field_arrays.insert(prop.clone());
                    }
                }
            }
        }
    }
}

/// Build the [`ClassDesc`] + the synthesized `HirFunc`s for one class. `parent`
/// is the already-built descriptor of the superclass (parent-first order), or
/// `None` for a root class.
pub(super) fn build_class(
    decl: &ClassDecl,
    parent: Option<&ClassDesc>,
) -> FrontResult<(ClassDesc, Vec<HirFunc>)> {
    let m = categorize(decl)?;
    let mut out: Vec<HirFunc> = Vec::new();

    // --- constructor (forwarding super if the subclass omits one) ---
    let (ctor_fn, ctor_arity, own_fields) = build_ctor(decl, &m, parent, &mut out)?;

    // --- FLATTENED field list: parent fields first, then own fields ---
    let mut fields: Vec<String> = parent.map(|p| p.fields.clone()).unwrap_or_default();
    for f in &own_fields {
        if !fields.iter().any(|x| x == f) {
            fields.push(f.clone());
        }
    }
    let global_shape = crate::shape::intern_global_shape(&fields);

    // --- FLATTENED array-typed fields: parent's set, then own array fields ---
    // A field is PROVEN to hold an array when its declaration initializer is an
    // array literal `[...]`, or a top-level `this.<field> = [...]` in the ctor.
    let mut field_arrays: std::collections::HashSet<String> =
        parent.map(|p| p.field_arrays.clone()).unwrap_or_default();
    for pd in &m.props {
        if let Some(init_expr) = &pd.initializer {
            let scope = Scope::new();
            let lowered = rts_hir::lower::lower_swc_expr(init_expr, &scope);
            if matches!(lowered.kind, HirExprKind::Array(_)) {
                field_arrays.insert(pd.name.clone());
            }
        }
    }
    // Ctor-assignment form: scan the OWN fields for a `this.<f> = [...]` whose RHS
    // is an array literal in the (already-lowered) constructor body.
    collect_ctor_array_fields(&out, &mut field_arrays);

    // --- instance methods (own) flattened over inherited ---
    let mut methods: HashMap<String, String> =
        parent.map(|p| p.methods.clone()).unwrap_or_default();
    for md in &m.methods {
        let fn_name = method_name(&decl.name, &md.name);
        out.push(synth_method(decl, md, parent, /*this*/ true)?);
        methods.insert(md.name.clone(), fn_name);
    }

    // --- accessors (own) flattened over inherited ---
    let mut accessors: HashMap<String, Accessor> =
        parent.map(|p| p.accessors.clone()).unwrap_or_default();
    for g in &m.getters {
        out.push(synth_method_named(decl, g, parent, true, getter_name(&decl.name, &g.name))?);
        accessors.entry(g.name.clone()).or_default().getter = Some(getter_name(&decl.name, &g.name));
    }
    for s in &m.setters {
        out.push(synth_method_named(decl, s, parent, true, setter_name(&decl.name, &s.name))?);
        accessors.entry(s.name.clone()).or_default().setter = Some(setter_name(&decl.name, &s.name));
    }

    // --- a field and an accessor of the same name is ambiguous: bail ---
    for f in &fields {
        if accessors.contains_key(f) {
            return Err(Unsupported::new(format!(
                "class `{}` declares both a field and an accessor named `{f}`",
                decl.name
            )));
        }
    }

    // --- statics ---
    let mut statics: HashMap<String, String> = HashMap::new();
    for sm in &m.static_methods {
        out.push(synth_static_method(decl, sm)?);
        statics.insert(sm.name.clone(), static_method_name(&decl.name, &sm.name));
    }
    let mut static_fields: HashMap<String, String> = HashMap::new();
    for sp in &m.static_props {
        out.push(synth_static_field_getter(decl, sp)?);
        static_fields.insert(sp.name.clone(), static_field_getter_name(&decl.name, &sp.name));
    }

    let desc = ClassDesc {
        name: decl.name.clone(),
        parent: decl.super_class.clone(),
        fields,
        global_shape,
        ctor: ctor_fn,
        ctor_arity,
        methods,
        accessors,
        statics,
        static_fields,
        field_arrays,
    };
    Ok((desc, out))
}

/// Split a class's members into the categories the synthesizer handles. A second
/// constructor / variadic-defaulted accessor and the like bail.
fn categorize(decl: &ClassDecl) -> FrontResult<Members<'_>> {
    let mut m = Members {
        ctor: None,
        methods: Vec::new(),
        getters: Vec::new(),
        setters: Vec::new(),
        static_methods: Vec::new(),
        props: Vec::new(),
        static_props: Vec::new(),
    };
    for mem in &decl.members {
        match mem {
            ClassMember::Constructor(c) => {
                if m.ctor.is_some() {
                    return Err(Unsupported::new(format!(
                        "class `{}` has more than one constructor",
                        decl.name
                    )));
                }
                m.ctor = Some(c);
            }
            ClassMember::Method(md) if md.modifiers.is_static => {
                if !matches!(md.role, MethodRole::Method) {
                    return Err(Unsupported::new(format!(
                        "static getter/setter `{}.{}`",
                        decl.name, md.name
                    )));
                }
                m.static_methods.push(md);
            }
            ClassMember::Method(md) => match md.role {
                MethodRole::Method => m.methods.push(md),
                MethodRole::Getter => m.getters.push(md),
                MethodRole::Setter => m.setters.push(md),
            },
            ClassMember::Property(pd) if pd.modifiers.is_static => m.static_props.push(pd),
            ClassMember::Property(pd) => m.props.push(pd),
        }
    }
    Ok(m)
}

/// Build the constructor `HirFunc`, returning `(fn_name, arity, own_field_names)`.
/// When the subclass omits a ctor but HAS a parent, a forwarding ctor is
/// synthesized (`__rtsn_ctor_C(this, ...parentParams){ super(...parentParams) }`).
fn build_ctor(
    decl: &ClassDecl,
    m: &Members,
    parent: Option<&ClassDesc>,
    out: &mut Vec<HirFunc>,
) -> FrontResult<(String, usize, Vec<String>)> {
    let mut scope = Scope::new();

    // The ctor's user params: the declared ones, OR (no ctor + a parent) the
    // parent's ctor arity forwarded under synthetic names `__a0..`.
    let (param_decls, forward): (Vec<HirParam>, Option<Vec<HirExpr>>) = match m.ctor {
        Some(c) => {
            let mut ps = Vec::new();
            for p in &c.parameters {
                // A REST param (`...xs`) is allowed (F3b); only a DEFAULTED param is
                // a later increment (needs rts-hir default threading).
                if p.default.is_some() {
                    return Err(Unsupported::new(format!(
                        "constructor of `{}` uses a defaulted parameter",
                        decl.name
                    )));
                }
                let ty = p
                    .type_annotation
                    .as_deref()
                    .map(rts_hir::lower::parse_type_annotation)
                    .unwrap_or(HirType::Unknown);
                scope.define(&p.name, ty.clone());
                ps.push(HirParam { name: p.name.clone(), ty, variadic: p.variadic, has_default: false, optional: false, default_expr: None });
            }
            (ps, None)
        }
        None => match parent {
            Some(p) => {
                // Forward the parent's ctor params positionally.
                let mut ps = Vec::new();
                let mut fwd = Vec::new();
                for i in 0..p.ctor_arity {
                    let name = format!("__a{i}");
                    scope.define(&name, HirType::Unknown);
                    ps.push(HirParam { name: name.clone(), ty: HirType::Unknown, variadic: false, has_default: false, optional: false, default_expr: None });
                    fwd.push(HirExpr::new(HirExprKind::Ident(name), HirType::Unknown));
                }
                (ps, Some(fwd))
            }
            None => (Vec::new(), None),
        },
    };
    let arity = param_decls.len();
    let mut params = vec![this_param()];
    params.extend(param_decls);

    // Field-init prologue from OWN declared property initializers (after super()).
    let mut prologue: Vec<HirStmt> = Vec::new();
    for pd in &m.props {
        if let Some(init_expr) = &pd.initializer {
            let value = rts_hir::lower::lower_swc_expr(init_expr, &scope);
            prologue.push(this_field_assign(&pd.name, value));
        }
    }

    // The body: either the user ctor body (super() already inside it) or the
    // synthesized `super(...forwarded)`. The property-init prologue runs after the
    // first statement IF that statement is a super() call (TS semantics); for
    // simplicity (and matching the field cases tested) we place initializers
    // after the implicit/explicit super.
    let mut body: Vec<HirStmt> = Vec::new();
    if let Some(fwd) = forward {
        // synthesized forwarding super(...).
        body.push(HirStmt::Expr(super_call_placeholder(fwd)));
    }
    if let Some(c) = m.ctor {
        let mut user = rts_hir::lower::lower_stmts(&c.body, &mut scope);
        rewrite_this_block(&mut user);
        rewrite_super_block(&mut user, parent)?;
        // Property initializers run AFTER super() but before the rest of the user
        // body. We approximate by appending the prologue right after a leading
        // super() statement if present, else at the front.
        splice_prologue(&mut user, prologue);
        body.extend(user);
    } else {
        body.extend(prologue);
    }
    // Rewrite the synthesized forwarding `super(...)` (and a no-op for an already
    // user-rewritten body). Idempotent: no `Raw("callee")` survives a first pass.
    rewrite_super_block(&mut body, parent)?;

    // OWN field order: declared props, then any extra `this.x` assigned in body.
    let mut own_fields: Vec<String> = Vec::new();
    for pd in &m.props {
        push_unique(&mut own_fields, &pd.name);
    }
    collect_this_assign_fields(&body, &mut own_fields);

    let ctor_fn_name = ctor_name(&decl.name);
    out.push(HirFunc {
        name: ctor_fn_name.clone(),
        params,
        ret: HirType::Void,
        body,
        is_async: false,
        is_arrow: false,
    });
    Ok((ctor_fn_name, arity, own_fields))
}

/// Build a `Raw("callee")`-callee Call so the synthesized forwarding `super(...)`
/// goes through the same [`super::inherit`] rewrite path as a user-written one.
fn super_call_placeholder(args: Vec<HirExpr>) -> HirExpr {
    HirExpr::new(
        HirExprKind::Call {
            callee: Box::new(HirExpr::new(HirExprKind::Raw("callee".to_string()), HirType::Unknown)),
            args,
        },
        HirType::Unknown,
    )
}

/// Place the property-init prologue right after a leading super() call (a Call to
/// a `__rtsn_ctor_*`), else at the front of the body.
fn splice_prologue(body: &mut Vec<HirStmt>, prologue: Vec<HirStmt>) {
    if prologue.is_empty() {
        return;
    }
    let after = matches!(body.first(), Some(HirStmt::Expr(e)) if is_super_ctor_call(e));
    let at = if after { 1 } else { 0 };
    for (i, s) in prologue.into_iter().enumerate() {
        body.insert(at + i, s);
    }
}

/// Whether a lowered expr is a synthesized super-ctor call (`__rtsn_ctor_*(...)`).
fn is_super_ctor_call(e: &HirExpr) -> bool {
    matches!(&e.kind, HirExprKind::Call { callee, .. }
        if matches!(&callee.kind, HirExprKind::Ident(n) if n.starts_with("__rtsn_ctor_")))
}

/// Synthesize an instance method `HirFunc` (name = `__rtsn_method_C_m`).
fn synth_method(
    decl: &ClassDecl,
    md: &MethodDecl,
    parent: Option<&ClassDesc>,
    with_this: bool,
) -> FrontResult<HirFunc> {
    synth_method_named(decl, md, parent, with_this, method_name(&decl.name, &md.name))
}

/// Synthesize a method-shaped fn under an explicit `fn_name` (used for methods,
/// getters, setters — all `this`-first instance functions).
fn synth_method_named(
    decl: &ClassDecl,
    md: &MethodDecl,
    parent: Option<&ClassDesc>,
    with_this: bool,
    fn_name: String,
) -> FrontResult<HirFunc> {
    let mut scope = Scope::new();
    let mut params: Vec<HirParam> = if with_this { vec![this_param()] } else { Vec::new() };
    for p in &md.parameters {
        // A REST param (`...xs`) is allowed (F3b); only a DEFAULTED param bails.
        if p.default.is_some() {
            return Err(Unsupported::new(format!(
                "method `{}.{}` uses a defaulted parameter",
                decl.name, md.name
            )));
        }
        let ty = p
            .type_annotation
            .as_deref()
            .map(rts_hir::lower::parse_type_annotation)
            .unwrap_or(HirType::Unknown);
        scope.define(&p.name, ty.clone());
        params.push(HirParam { name: p.name.clone(), ty, variadic: p.variadic, has_default: false, optional: false, default_expr: None });
    }
    // A setter returns nothing (its call is a statement); model it `Void` so the
    // sig is value-less and a fall-through body is well-formed.
    let ret = if matches!(md.role, MethodRole::Setter) {
        HirType::Void
    } else {
        md.return_type
            .as_deref()
            .map(rts_hir::lower::parse_type_annotation)
            .unwrap_or(HirType::Unknown)
    };
    let mut body = rts_hir::lower::lower_stmts(&md.body, &mut scope);
    rewrite_this_block(&mut body);
    rewrite_super_block(&mut body, parent)?;
    Ok(HirFunc { name: fn_name, params, ret, body, is_async: false, is_arrow: false })
}

/// Synthesize a static method `HirFunc` (NO `this`; name = `__rtsn_static_C_m`).
/// A `this` reference inside a static method bails (no instance receiver).
fn synth_static_method(decl: &ClassDecl, md: &MethodDecl) -> FrontResult<HirFunc> {
    let mut scope = Scope::new();
    let mut params: Vec<HirParam> = Vec::new();
    for p in &md.parameters {
        // A REST param (`...xs`) is allowed (F3b); only a DEFAULTED param bails.
        if p.default.is_some() {
            return Err(Unsupported::new(format!(
                "static method `{}.{}` uses a defaulted parameter",
                decl.name, md.name
            )));
        }
        let ty = p
            .type_annotation
            .as_deref()
            .map(rts_hir::lower::parse_type_annotation)
            .unwrap_or(HirType::Unknown);
        scope.define(&p.name, ty.clone());
        params.push(HirParam { name: p.name.clone(), ty, variadic: p.variadic, has_default: false, optional: false, default_expr: None });
    }
    let ret = md
        .return_type
        .as_deref()
        .map(rts_hir::lower::parse_type_annotation)
        .unwrap_or(HirType::Unknown);
    let body = rts_hir::lower::lower_stmts(&md.body, &mut scope);
    // A `this` in a static method has no instance receiver — refuse it (sound).
    if body_uses_this(&body) {
        return Err(Unsupported::new(format!(
            "`this` inside static method `{}.{}` (no instance receiver)",
            decl.name, md.name
        )));
    }
    Ok(HirFunc {
        name: static_method_name(&decl.name, &md.name),
        params,
        ret,
        body,
        is_async: false,
        is_arrow: false,
    })
}

/// Synthesize a zero-arg getter fn returning a static field's initializer
/// (`__rtsn_sfield_C_f() { return <init>; }`). A static field with no initializer,
/// or an initializer referencing `this`/other statics, bails.
fn synth_static_field_getter(decl: &ClassDecl, pd: &PropertyDecl) -> FrontResult<HirFunc> {
    let scope = Scope::new();
    let Some(init_expr) = &pd.initializer else {
        return Err(Unsupported::new(format!(
            "static field `{}.{}` without an initializer",
            decl.name, pd.name
        )));
    };
    let value = rts_hir::lower::lower_swc_expr(init_expr, &scope);
    let body = vec![HirStmt::Return(Some(value))];
    if body_uses_this(&body) {
        return Err(Unsupported::new(format!(
            "static field `{}.{}` initializer references `this`",
            decl.name, pd.name
        )));
    }
    Ok(HirFunc {
        name: static_field_getter_name(&decl.name, &pd.name),
        params: Vec::new(),
        ret: HirType::Unknown,
        body,
        is_async: false,
        is_arrow: false,
    })
}
