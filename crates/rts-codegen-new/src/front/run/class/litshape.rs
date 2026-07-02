//! Object-literal METHODS as a synthesized "literal class" (P5.15).
//!
//! An object literal `{ field: v, method() {…}, toString() {…} }` keeps its
//! FIELDS in the P3.6 slot layout (slot 0 = global shape-id, fields at 1+idx),
//! exactly as a fieldful literal. Its METHODS, which rts-hir drops entirely from
//! the `Object` node, are RECOVERED from the swc AST (see
//! [`super::super::desugar::objmethod`]) and synthesized here into a [`ClassDesc`]
//! — reusing the SAME machinery a real class uses: each method is a top-level
//! `HirFunc` whose first parameter is `this`, with swc's `Raw("This(…)")` rewritten
//! to `Ident("this")`, and `this.field` resolving against the literal's flat shape.
//!
//! The descriptor has NO constructor (the literal is built inline by
//! [`super::super::obj::Lowerer::lower_object_literal`], not via a `new`); its
//! `ctor` name is a never-defined placeholder. With the descriptor registered in
//! the program's [`super::ClassTable`] and the literal's local recorded in
//! `local_classes`, `obj.method(args)` static-dispatches and `${obj}` / `obj + 1`
//! / `String(obj)` run the `toString`/`valueOf` ToPrimitive chain — all through the
//! EXISTING class-instance paths, with zero new dispatch logic.
//!
//! Soundness: only PLAIN methods (non-generator, non-async, non-computed name) are
//! recovered. A getter/setter/computed/generator/async method, or a spread, makes
//! the caller skip recovery entirely — the literal keeps no class and any method
//! call on it BAILS (never a guess).

use rts_ast::ast::{RawStmt, Statement};
use rts_ast::span::Span as AstSpan;
use rts_hir::ir::{HirFunc, HirParam, HirType};
use rts_hir::scope::Scope;

use std::collections::HashMap;

use super::synth::method_name_lit;
use super::walk::rewrite_this_block;
use super::{ClassDesc, this_param};

/// One recovered plain method of an object literal: its name + the swc function.
pub(crate) struct LitMethod<'a> {
    pub name: String,
    pub fn_ref: LitFnRef<'a>,
}

/// The swc node backing a literal member: a plain METHOD, or a GETTER/SETTER
/// (recovered into the literal class's `accessors` map, dispatched exactly like
/// a user class accessor).
pub(crate) enum LitFnRef<'a> {
    Method(&'a swc_ecma_ast::Function),
    Getter(&'a swc_ecma_ast::GetterProp),
    Setter(&'a swc_ecma_ast::SetterProp),
}

/// Build the literal-class [`ClassDesc`] + its synthesized method `HirFunc`s for an
/// object literal whose ordered FIELD keys are `fields` and whose recovered plain
/// methods are `methods`. `class_name` is the synthetic, content-keyed name the
/// recovery pass assigned (so two identical literals share one descriptor).
///
/// `global_shape` MUST be the same global shape-id the literal bakes into slot 0
/// (interned from `fields`), so `this.field` reads land on the right slot.
pub(crate) fn build_literal_class(
    class_name: &str,
    fields: &[String],
    global_shape: u32,
    methods: &[LitMethod<'_>],
) -> (ClassDesc, Vec<HirFunc>) {
    let mut out: Vec<HirFunc> = Vec::new();
    let mut method_map: HashMap<String, String> = HashMap::new();
    let mut accessors: HashMap<String, super::Accessor> = HashMap::new();

    for m in methods {
        match &m.fn_ref {
            LitFnRef::Method(f) => {
                let fn_name = method_name_lit(class_name, &m.name);
                out.push(synth_lit_method(&fn_name, &f.params, f.body.as_ref()));
                method_map.insert(m.name.clone(), fn_name);
            }
            LitFnRef::Getter(g) => {
                let fn_name = format!("__rtsn_lget_{class_name}_{}", m.name);
                out.push(synth_lit_method(&fn_name, &[], g.body.as_ref()));
                accessors.entry(m.name.clone()).or_insert(super::Accessor {
                    getter: None,
                    setter: None,
                }).getter = Some(fn_name);
            }
            LitFnRef::Setter(s) => {
                let fn_name = format!("__rtsn_lset_{class_name}_{}", m.name);
                let param = swc_ecma_ast::Param {
                    span: Default::default(),
                    decorators: Vec::new(),
                    pat: (*s.param).clone(),
                };
                out.push(synth_lit_method(&fn_name, std::slice::from_ref(&param), s.body.as_ref()));
                accessors.entry(m.name.clone()).or_insert(super::Accessor {
                    getter: None,
                    setter: None,
                }).setter = Some(fn_name);
            }
        }
    }

    let desc = ClassDesc {
        name: class_name.to_string(),
        parent: None,
        fields: fields.to_vec(),
        global_shape,
        // No real constructor: the literal is built inline, never via `new`. The
        // name is a placeholder that is NEVER declared/defined as a function.
        ctor: format!("__rtsl_noctor_{class_name}"),
        ctor_arity: 0,
        methods: method_map,
        accessors,
        statics: HashMap::new(),
        static_fields: HashMap::new(),
        // Object-literal classes do not track array-typed fields (no `this.x[i]`
        // chaining target in the literal-recovery path yet).
        field_arrays: std::collections::HashSet::new(),
        // Object-literal fields have no declared type annotation to prove `string`.
        field_strings: std::collections::HashSet::new(),
    };
    (desc, out)
}

/// Synthesize one literal method as a `this`-first `HirFunc`, mirroring
/// [`super::synth`]'s class-method synthesis but driven by a raw swc `Function`
/// (the body rts-hir dropped). Generator/async/computed cases are filtered by the
/// recovery pass before reaching here.
fn synth_lit_method(
    fn_name: &str,
    swc_params: &[swc_ecma_ast::Param],
    body: Option<&swc_ecma_ast::BlockStmt>,
) -> HirFunc {
    let mut scope = Scope::new();
    let mut params: Vec<HirParam> = vec![this_param()];
    for p in swc_params {
        // Only a simple identifier parameter is recovered; the recovery pass refused
        // any non-ident / defaulted / rest param before building this method.
        let name = ident_param_name(&p.pat).expect("recovery proved a simple ident param");
        scope.define(&name, HirType::Unknown);
        params.push(HirParam {
            name,
            ty: HirType::Unknown,
            variadic: false,
            has_default: false,
            optional: false,
            default_expr: None,
        });
    }

    // Lower the swc body by wrapping each statement as an rts-ast `Raw` (the same
    // bridge the parser uses), then run rts-hir's statement lowering against the
    // method scope. An empty body lowers to an empty block.
    let stmts: Vec<Statement> = body
        .map(|b| b.stmts.iter().map(wrap_stmt).collect())
        .unwrap_or_default();
    let mut body = rts_hir::lower::lower_stmts(&stmts, &mut scope);
    rewrite_this_block(&mut body);

    HirFunc {
        name: fn_name.to_string(),
        params,
        ret: HirType::Unknown,
        body,
        is_async: false,
        is_arrow: false,
    }
}

/// Wrap a swc statement into an rts-ast `Statement::Raw` carrying the parsed node
/// (exactly how `rts-parser` feeds rts-hir), so `lower_stmts` consumes it directly.
fn wrap_stmt(s: &swc_ecma_ast::Stmt) -> Statement {
    Statement::Raw(RawStmt {
        text: String::new(),
        span: AstSpan::default(),
        stmt: Some(s.clone()),
    })
}

/// The bound name of a simple identifier parameter pattern, or `None` for any
/// destructuring / rest / defaulted / `this`-typed parameter.
pub(crate) fn ident_param_name(pat: &swc_ecma_ast::Pat) -> Option<String> {
    match pat {
        swc_ecma_ast::Pat::Ident(b) => Some(b.id.sym.to_string()),
        _ => None,
    }
}
