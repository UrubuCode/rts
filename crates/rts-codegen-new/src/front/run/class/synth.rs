//! Synthesize a class's [`ClassDesc`] + its constructor/method `HirFunc`s from
//! the AST `ClassDecl` (P4.9).
//!
//! The constructor and every method become ordinary top-level `HirFunc`s whose
//! FIRST parameter is the implicit receiver `this`:
//!
//! ```text
//! class C { x: number; constructor(a: number){ this.x = a; } get2(){ return this.x*2; } }
//!   =>  fn __rtsn_ctor_C(this, a: number) { this.x = a; }   // returns void
//!       fn __rtsn_method_C_get2(this)     { return this.x*2; }
//! ```
//!
//! `new C(args)` (lowered in `front/run/new.rs`) allocates the instance Vec (slot
//! 0 = the class's global shape-id, one `undefined` slot per field), calls the
//! constructor with the instance as `this`, and yields the instance.
//!
//! The field ORDER is `declared properties ∪ first-assigned this.x in the
//! constructor`, in first-seen order; the global shape-id is interned from that
//! list so every instance shares one shape (and the inspect trampoline recovers
//! the keys). swc lowers `this` to a `Raw("This(...)")` node, so after lowering
//! each body we rewrite those to `Ident("this")` (the lowerer binds `this` to a0).

use rts_ast::ast::{ClassDecl, ClassMember, ConstructorDecl, MethodDecl, PropertyDecl};
use rts_hir::ir::{HirExpr, HirExprKind, HirFunc, HirParam, HirStmt, HirType};
use rts_hir::scope::Scope;

use crate::front::error::{FrontResult, Unsupported};

use super::{this_param, ClassDesc, THIS};

/// The synthesized constructor function name for class `name`.
pub(super) fn ctor_name(name: &str) -> String {
    format!("__rtsn_ctor_{name}")
}

/// The synthesized method function name for `class.method`.
pub(super) fn method_name(class: &str, method: &str) -> String {
    format!("__rtsn_method_{class}_{method}")
}

/// Build the [`ClassDesc`] + the constructor/method `HirFunc`s for one class.
pub(super) fn build_class(decl: &ClassDecl) -> FrontResult<(ClassDesc, Vec<HirFunc>)> {
    let mut scope = Scope::new();

    // --- gather the constructor (if any) and the method/property members ---
    let mut ctor: Option<&ConstructorDecl> = None;
    let mut methods: Vec<&MethodDecl> = Vec::new();
    let mut props: Vec<&PropertyDecl> = Vec::new();
    for m in &decl.members {
        match m {
            ClassMember::Constructor(c) => {
                if ctor.is_some() {
                    return Err(Unsupported::new(format!(
                        "class `{}` has more than one constructor",
                        decl.name
                    )));
                }
                ctor = Some(c);
            }
            ClassMember::Method(md) => methods.push(md),
            ClassMember::Property(pd) => props.push(pd),
        }
    }

    // --- constructor params (after the implicit `this`) ---
    let ctor_params = ctor.map(|c| &c.parameters[..]).unwrap_or(&[]);
    let mut params: Vec<HirParam> = vec![this_param()];
    for p in ctor_params {
        let ty = p
            .type_annotation
            .as_deref()
            .map(rts_hir::lower::parse_type_annotation)
            .unwrap_or(HirType::Unknown);
        scope.define(&p.name, ty.clone());
        if p.variadic || p.default.is_some() {
            return Err(Unsupported::new(format!(
                "constructor of `{}` uses a variadic / defaulted parameter",
                decl.name
            )));
        }
        params.push(HirParam { name: p.name.clone(), ty, variadic: false, has_default: false });
    }
    let ctor_arity = ctor_params.len();

    // --- field-init prologue from declared property initializers ---
    // `x: T = init;` becomes `this.x = init;` at the top of the constructor body.
    let mut prologue: Vec<HirStmt> = Vec::new();
    for pd in &props {
        if let Some(init_expr) = &pd.initializer {
            let value = rts_hir::lower::lower_swc_expr(init_expr, &scope);
            prologue.push(this_field_assign(&pd.name, value));
        }
    }

    // --- lower the constructor body, rewrite `this`, prepend the prologue ---
    let mut ctor_body: Vec<HirStmt> = prologue;
    if let Some(c) = ctor {
        let mut body = rts_hir::lower::lower_stmts(&c.body, &mut scope);
        rewrite_this_block(&mut body);
        ctor_body.extend(body);
    }

    // --- field ORDER: declared props, then any extra this.x assigned in ctor ---
    let mut fields: Vec<String> = Vec::new();
    for pd in &props {
        push_unique(&mut fields, &pd.name);
    }
    collect_this_assign_fields(&ctor_body, &mut fields);

    // --- intern the global shape-id from the field list ---
    let global_shape = crate::shape::intern_global_shape(&fields);

    // --- synthesize the constructor HirFunc (void return) ---
    let ctor_fn_name = ctor_name(&decl.name);
    let mut out: Vec<HirFunc> = Vec::new();
    out.push(HirFunc {
        name: ctor_fn_name.clone(),
        params,
        ret: HirType::Void,
        body: ctor_body,
        is_async: false,
        is_arrow: false,
    });

    // --- synthesize each method HirFunc (`this` + the method's own params) ---
    let mut method_map = std::collections::HashMap::new();
    for md in &methods {
        let fn_name = method_name(&decl.name, &md.name);
        let mut mscope = Scope::new();
        let mut mparams: Vec<HirParam> = vec![this_param()];
        for p in &md.parameters {
            if p.variadic || p.default.is_some() {
                return Err(Unsupported::new(format!(
                    "method `{}.{}` uses a variadic / defaulted parameter",
                    decl.name, md.name
                )));
            }
            let ty = p
                .type_annotation
                .as_deref()
                .map(rts_hir::lower::parse_type_annotation)
                .unwrap_or(HirType::Unknown);
            mscope.define(&p.name, ty.clone());
            mparams.push(HirParam { name: p.name.clone(), ty, variadic: false, has_default: false });
        }
        let ret = md
            .return_type
            .as_deref()
            .map(rts_hir::lower::parse_type_annotation)
            .unwrap_or(HirType::Unknown);
        let mut body = rts_hir::lower::lower_stmts(&md.body, &mut mscope);
        rewrite_this_block(&mut body);
        out.push(HirFunc {
            name: fn_name.clone(),
            params: mparams,
            ret,
            body,
            is_async: false,
            is_arrow: false,
        });
        if method_map.insert(md.name.clone(), fn_name).is_some() {
            return Err(Unsupported::new(format!(
                "class `{}` declares method `{}` twice",
                decl.name, md.name
            )));
        }
    }

    let desc = ClassDesc {
        name: decl.name.clone(),
        fields,
        global_shape,
        ctor: ctor_fn_name,
        ctor_arity,
        methods: method_map,
    };
    Ok((desc, out))
}

/// `this.<field> = <value>` as an HIR statement (an `Assign` to a `Member` on the
/// `this` identifier). Used for property initializers in the constructor prologue.
fn this_field_assign(field: &str, value: HirExpr) -> HirStmt {
    let this = HirExpr::new(HirExprKind::Ident(THIS.to_string()), HirType::Unknown);
    let member = HirExpr::new(
        HirExprKind::Member { object: Box::new(this), prop: field.to_string() },
        HirType::Unknown,
    );
    HirStmt::Expr(HirExpr::new(
        HirExprKind::Assign { target: Box::new(member), value: Box::new(value) },
        HirType::Unknown,
    ))
}

/// Append `name` to `fields` if not already present (first-seen-order union).
fn push_unique(fields: &mut Vec<String>, name: &str) {
    if !fields.iter().any(|f| f == name) {
        fields.push(name.to_string());
    }
}

/// Walk a (constructor) HIR body collecting every `this.<field> = …` target field
/// not already in `fields`, in first-seen order (a field assigned but not
/// declared as a property still becomes an instance slot).
fn collect_this_assign_fields(stmts: &[HirStmt], fields: &mut Vec<String>) {
    for s in stmts {
        walk_stmt_fields(s, fields);
    }
}

fn walk_stmt_fields(s: &HirStmt, fields: &mut Vec<String>) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => walk_expr_fields(e, fields),
        HirStmt::Return(Some(e)) => walk_expr_fields(e, fields),
        HirStmt::Let { init: Some(e), .. } => walk_expr_fields(e, fields),
        HirStmt::Const { init, .. } => walk_expr_fields(init, fields),
        HirStmt::If { cond, then, else_ } => {
            walk_expr_fields(cond, fields);
            then.iter().for_each(|s| walk_stmt_fields(s, fields));
            if let Some(e) = else_ {
                e.iter().for_each(|s| walk_stmt_fields(s, fields));
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            walk_expr_fields(cond, fields);
            body.iter().for_each(|s| walk_stmt_fields(s, fields));
        }
        HirStmt::Block(b) => b.iter().for_each(|s| walk_stmt_fields(s, fields)),
        HirStmt::For { body, .. } | HirStmt::ForOf { body, .. } | HirStmt::ForIn { body, .. } => {
            body.iter().for_each(|s| walk_stmt_fields(s, fields));
        }
        _ => {}
    }
}

fn walk_expr_fields(e: &HirExpr, fields: &mut Vec<String>) {
    if let HirExprKind::Assign { target, value } = &e.kind {
        if let HirExprKind::Member { object, prop } = &target.kind {
            if matches!(&object.kind, HirExprKind::Ident(n) if n == THIS) {
                push_unique(fields, prop);
            }
        }
        walk_expr_fields(value, fields);
        return;
    }
    // Descend into common compound expressions so a `this.x = …` nested in an
    // initializer / ternary is still discovered.
    match &e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            walk_expr_fields(lhs, fields);
            walk_expr_fields(rhs, fields);
        }
        HirExprKind::AssignOp { value, .. } => walk_expr_fields(value, fields),
        HirExprKind::Call { args, .. } | HirExprKind::MethodCall { args, .. } => {
            args.iter().for_each(|a| walk_expr_fields(a, fields));
        }
        HirExprKind::Ternary { then, else_, .. } => {
            walk_expr_fields(then, fields);
            walk_expr_fields(else_, fields);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// `this` rewriting: swc lowers `this` to a Raw node; turn each into Ident("this").
// ---------------------------------------------------------------------------

fn rewrite_this_block(stmts: &mut [HirStmt]) {
    for s in stmts {
        rewrite_this_stmt(s);
    }
}

fn rewrite_this_stmt(s: &mut HirStmt) {
    match s {
        HirStmt::Expr(e) | HirStmt::Throw(e) => rewrite_this_expr(e),
        HirStmt::Return(opt) => {
            if let Some(e) = opt {
                rewrite_this_expr(e);
            }
        }
        HirStmt::Let { init, .. } => {
            if let Some(e) = init {
                rewrite_this_expr(e);
            }
        }
        HirStmt::Const { init, .. } => rewrite_this_expr(init),
        HirStmt::If { cond, then, else_ } => {
            rewrite_this_expr(cond);
            rewrite_this_block(then);
            if let Some(e) = else_ {
                rewrite_this_block(e);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { cond, body } => {
            rewrite_this_expr(cond);
            rewrite_this_block(body);
        }
        HirStmt::For { cond, update, body, .. } => {
            if let Some(c) = cond {
                rewrite_this_expr(c);
            }
            if let Some(u) = update {
                rewrite_this_expr(u);
            }
            rewrite_this_block(body);
        }
        HirStmt::ForOf { iterable, body, .. } => {
            rewrite_this_expr(iterable);
            rewrite_this_block(body);
        }
        HirStmt::ForIn { object, body, .. } => {
            rewrite_this_expr(object);
            rewrite_this_block(body);
        }
        HirStmt::Block(b) => rewrite_this_block(b),
        _ => {}
    }
}

fn rewrite_this_expr(e: &mut HirExpr) {
    if super::is_raw_this(e) {
        e.kind = HirExprKind::Ident(THIS.to_string());
        e.ty = HirType::Unknown;
        return;
    }
    match &mut e.kind {
        HirExprKind::Bin { lhs, rhs, .. } => {
            rewrite_this_expr(lhs);
            rewrite_this_expr(rhs);
        }
        HirExprKind::Unary { operand, .. } => rewrite_this_expr(operand),
        HirExprKind::Assign { target, value } | HirExprKind::AssignOp { target, value, .. } => {
            rewrite_this_expr(target);
            rewrite_this_expr(value);
        }
        HirExprKind::Call { callee, args } => {
            rewrite_this_expr(callee);
            args.iter_mut().for_each(rewrite_this_expr);
        }
        HirExprKind::MethodCall { object, args, .. } => {
            rewrite_this_expr(object);
            args.iter_mut().for_each(rewrite_this_expr);
        }
        HirExprKind::Member { object, .. } => rewrite_this_expr(object),
        HirExprKind::Index { object, index } => {
            rewrite_this_expr(object);
            rewrite_this_expr(index);
        }
        HirExprKind::Ternary { cond, then, else_ } => {
            rewrite_this_expr(cond);
            rewrite_this_expr(then);
            rewrite_this_expr(else_);
        }
        HirExprKind::Array(elems) => elems.iter_mut().for_each(rewrite_this_expr),
        HirExprKind::Object(fields) => fields.iter_mut().for_each(|(_, v)| rewrite_this_expr(v)),
        HirExprKind::Await(inner) | HirExprKind::Spread(inner) | HirExprKind::Cast { expr: inner, .. } => {
            rewrite_this_expr(inner)
        }
        HirExprKind::PreInc(t) | HirExprKind::PreDec(t) | HirExprKind::PostInc(t) | HirExprKind::PostDec(t) => {
            rewrite_this_expr(t)
        }
        HirExprKind::Seq(items) => items.iter_mut().for_each(rewrite_this_expr),
        HirExprKind::New { args, .. } => args.iter_mut().for_each(rewrite_this_expr),
        _ => {}
    }
}
