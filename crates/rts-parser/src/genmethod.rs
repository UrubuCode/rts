//! Class generator methods (`class C { *m() {} }`) -> top-level generator
//! declarations, so the lazy state machine can reach them.
//!
//! ## Why this exists
//!
//! `lowering_items::lower_decl`'s `Decl::Fn` arm tries
//! [`crate::generator_sm::try_build`] FIRST and only falls back to the eager
//! buffer when the body is ineligible. `lowering_decls`'s class-member arms
//! never had that step: a `is_generator` method went straight to
//! [`crate::generator_desugar::desugar_generator_body`].
//!
//! The eager buffer only expresses `yield` in STATEMENT position (it becomes
//! `__gen_buf.push`). A `yield` in VALUE position (`const r = yield x`, which
//! needs the value a later `.next(v)` sends back) survives the desugar and
//! reaches the lowering as a raw node, where it dies with
//! `expression raw/unrecognized: Yield(...)`. That is the failure real bundles
//! hit — the `__rtsn_ctor___genexpr_*` names in the reports are class-member
//! generators.
//!
//! ## The transform
//!
//! Rather than teach the state machine about `this` (it has no notion of it),
//! the method is rewritten into a shape the EXISTING top-level path already
//! handles:
//!
//! ```text
//! class C { *m(a) { const r = yield this.v; } }
//! ```
//! becomes
//! ```text
//! function* __genmethod_C_m_0(__gen_this: i64, a) { const r = yield __gen_this.v; }
//! class C { m(a) { return __genmethod_C_m_0(this, a); } }
//! ```
//!
//! The hoisted declaration is fed back through `lower_decl`, so it goes through
//! `try_build` exactly like any other top-level `function*` — no second
//! dispatch, no duplicated eligibility rules. When `try_build` declines, the
//! hoisted declaration falls to the eager buffer, which is what the method
//! would have gotten anyway.
//!
//! `__gen_this` is annotated `i64` deliberately: it carries a class-instance
//! HANDLE, and an unannotated parameter receiving a handle is corrupted on the
//! way in (issue #1870).
//!
//! ## What it declines (and why declining is safe)
//!
//! [`eligible`] bails on anything whose meaning is tied to the class body, so
//! the member keeps TODAY's behaviour instead of being moved somewhere the name
//! cannot resolve:
//!
//! - `super.x` / `super()` — no super binding at top level;
//! - any private name (`this.#f`, `#f in o`) — lexically class-scoped;
//! - `arguments` and `new.target` — bound by the original call, not the wrapper;
//! - non-identifier parameters (destructuring/rest) — the wrapper forwards by
//!   name, so there is no name to forward;
//! - computed keys — no stable name to build the hoisted symbol from;
//! - `async *m()` — the async-generator path has its own driver; out of scope
//!   for this slice, left on the existing path.

use std::cell::Cell;

use swc_common::{DUMMY_SP, Span};
use swc_ecma_ast::{
    BlockStmt, Callee, Class, ClassDecl, ClassMember, Expr, ExprOrSpread, FnDecl, Function, Ident,
    MemberProp, Param, Pat, PropName, ReturnStmt, Stmt, ThisExpr, TsEntityName, TsType, TsTypeAnn,
    TsTypeRef,
};
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

/// Name of the synthetic leading parameter carrying the receiver.
const THIS_PARAM: &str = "__gen_this";

thread_local! {
    /// Monotonic id so two classes with the same name (or a static and an
    /// instance member sharing a name) never collide on the hoisted symbol.
    static COUNTER: Cell<u32> = const { Cell::new(0) };

    /// Real span of the member body being rewritten. EVERY synthetic node uses
    /// it: the lowering gates a statement on `span_snippet` returning non-empty
    /// text, so a `DUMMY_SP` node is silently DISCARDED — the wrapper compiled
    /// to a bare `return undefined` and the iterator protocol vanished with it.
    /// `generator_sm` keeps its own `SM_SPAN` for exactly this reason.
    static SPAN: Cell<Span> = const { Cell::new(DUMMY_SP) };
}

fn sp() -> Span {
    SPAN.with(|c| c.get())
}

fn next_id() -> u32 {
    COUNTER.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}

/// What [`hoist_generator_methods`] produced.
#[derive(Default)]
pub struct Hoisted {
    /// Top-level generator declarations to lower before the class.
    pub decls: Vec<FnDecl>,
    /// Member names (as `lowering_decls` spells them — private methods carry the
    /// leading `#`) whose body is now a wrapper. The caller re-stamps their
    /// return type, which `lowering_decls` only forces for `is_generator`
    /// members and the wrapper is deliberately no longer one.
    pub wrapper_methods: Vec<String>,
}

/// Rewrite every eligible generator member of `class_decl` into a wrapper and
/// return the generator declarations to hoist. Ineligible members are left
/// untouched, keeping their current behaviour.
pub fn hoist_generator_methods(class_decl: &mut ClassDecl) -> Hoisted {
    let class_name = class_decl.ident.sym.to_string();
    let mut out = Hoisted::default();

    for member in class_decl.class.body.iter_mut() {
        match member {
            ClassMember::Method(m) if m.function.is_generator => {
                let Some(mname) = simple_prop_name(&m.key) else {
                    continue;
                };
                let is_static = m.is_static;
                if let Some(fd) = rewrite(&class_name, &mname, &mut m.function, is_static) {
                    out.decls.push(fd);
                    out.wrapper_methods.push(mname);
                }
            }
            ClassMember::PrivateMethod(m) if m.function.is_generator => {
                let mname = m.key.name.to_string();
                // `P_` keeps the hoisted symbol distinct from a public member of
                // the same name; the wrapper is still looked up as `#name`.
                let is_static = m.is_static;
                if let Some(fd) =
                    rewrite(&class_name, &format!("P_{mname}"), &mut m.function, is_static)
                {
                    out.decls.push(fd);
                    out.wrapper_methods.push(format!("#{mname}"));
                }
            }
            _ => {}
        }
    }
    out
}

/// Turn `f` into the forwarding wrapper and return the hoisted generator.
/// `None` leaves `f` exactly as it was.
fn rewrite(
    class_name: &str,
    member_name: &str,
    f: &mut Function,
    is_static: bool,
) -> Option<FnDecl> {
    let Some(body_span) = f.body.as_ref().map(|b| b.span) else {
        return None;
    };
    if f.is_async || !eligible(f) {
        return None;
    }
    // A STATIC method has no instance receiver, and the engine already refuses
    // `this` there ("`this` inside static method ... (no instance receiver)").
    // So a static generator is hoisted WITHOUT the receiver parameter — and one
    // whose body mentions `this` is left alone rather than moved into a shape
    // that would bail anyway.
    if is_static && mentions_this(f) {
        return None;
    }
    SPAN.with(|c| c.set(body_span));
    let forward = param_names(&f.params)?;

    let hoisted_name = format!(
        "__genmethod_{}_{}_{}",
        sanitize(class_name),
        sanitize(member_name),
        next_id()
    );

    // ── the hoisted generator: `this` becomes the leading parameter ──────────
    let mut hoisted = f.clone();
    if !is_static {
        let mut rw = ThisRewriter;
        // Parameter defaults are rewritten too: `__gen_this` is parameter 0, so a
        // default that referenced `this` still resolves (and in the right order).
        for p in hoisted.params.iter_mut() {
            p.visit_mut_with(&mut rw);
        }
        if let Some(b) = hoisted.body.as_mut() {
            b.visit_mut_with(&mut rw);
        }
        hoisted.params.insert(0, this_param());
    }
    hoisted.return_type = None;
    hoisted.is_generator = true;

    // ── the wrapper: `return __genmethod_...(this, ...params);` ──────────────
    let mut args: Vec<ExprOrSpread> = Vec::with_capacity(forward.len() + 1);
    if !is_static {
        args.push(arg(Expr::This(ThisExpr { span: sp() })));
    }
    for name in &forward {
        args.push(arg(ident_expr(name)));
    }
    let call = Expr::Call(swc_ecma_ast::CallExpr {
        span: sp(),
        ctxt: Default::default(),
        callee: Callee::Expr(Box::new(ident_expr(&hoisted_name))),
        args,
        type_args: None,
    });
    f.is_generator = false;
    f.return_type = None;
    f.body = Some(BlockStmt {
        span: sp(),
        ctxt: Default::default(),
        stmts: vec![Stmt::Return(ReturnStmt {
            span: sp(),
            arg: Some(Box::new(call)),
        })],
    });

    Some(FnDecl {
        ident: ident(&hoisted_name),
        declare: false,
        function: Box::new(hoisted),
    })
}

/// `__gen_this: i64` — the annotation is load-bearing, see the module docs.
fn this_param() -> Param {
    let mut bi: swc_ecma_ast::BindingIdent = ident(THIS_PARAM).into();
    bi.type_ann = Some(Box::new(TsTypeAnn {
        span: sp(),
        type_ann: Box::new(TsType::TsTypeRef(TsTypeRef {
            span: sp(),
            type_name: TsEntityName::Ident(ident("i64")),
            type_params: None,
        })),
    }));
    Param {
        span: sp(),
        decorators: Vec::new(),
        pat: Pat::Ident(bi),
    }
}

/// The names the wrapper forwards. `None` when a parameter has no single name
/// to forward (destructuring, rest).
fn param_names(params: &[Param]) -> Option<Vec<String>> {
    let mut out = Vec::with_capacity(params.len());
    for p in params {
        match &p.pat {
            Pat::Ident(id) => out.push(id.id.sym.to_string()),
            // `m(a = 1)`: the wrapper keeps the default and forwards the value
            // it bound, so the name is still the identifier on the left.
            Pat::Assign(a) => match a.left.as_ref() {
                Pat::Ident(id) => out.push(id.id.sym.to_string()),
                _ => return None,
            },
            _ => return None,
        }
    }
    Some(out)
}

/// `this` -> `__gen_this`, without descending where `this` rebinds.
struct ThisRewriter;

impl VisitMut for ThisRewriter {
    fn visit_mut_expr(&mut self, e: &mut Expr) {
        if matches!(e, Expr::This(_)) {
            *e = ident_expr(THIS_PARAM);
            return;
        }
        e.visit_mut_children_with(self);
    }

    /// A nested non-arrow `function` gets its OWN `this`. Arrow functions
    /// inherit it, and are reached through the default `visit_mut_expr` walk.
    fn visit_mut_function(&mut self, _f: &mut Function) {}

    /// A nested class body binds its own `this` in its methods.
    fn visit_mut_class(&mut self, _c: &mut Class) {}
}

/// True when the body mentions `this` at a depth where it would still bind to
/// the method's receiver (nested non-arrow functions and classes rebind it).
fn mentions_this(f: &Function) -> bool {
    struct T {
        found: bool,
    }
    impl Visit for T {
        fn visit_expr(&mut self, e: &Expr) {
            if matches!(e, Expr::This(_)) {
                self.found = true;
                return;
            }
            e.visit_children_with(self);
        }
        fn visit_function(&mut self, _f: &Function) {}
        fn visit_class(&mut self, _c: &Class) {}
    }
    let mut t = T { found: false };
    // `visit_function` is suppressed, so drive the body/params directly.
    if let Some(b) = f.body.as_ref() {
        b.visit_with(&mut t);
    }
    for p in &f.params {
        p.visit_with(&mut t);
    }
    t.found
}

/// True when the body can be moved out of the class body unchanged.
fn eligible(f: &Function) -> bool {
    let mut scan = Scan { bad: false };
    f.visit_with(&mut scan);
    !scan.bad
}

struct Scan {
    bad: bool,
}

impl Visit for Scan {
    fn visit_expr(&mut self, e: &Expr) {
        match e {
            // `super.x`, `new.target`, and a bare `#f` (as in `#f in obj`) all
            // mean something only inside the class body.
            Expr::SuperProp(_) | Expr::MetaProp(_) | Expr::PrivateName(_) => {
                self.bad = true;
                return;
            }
            Expr::Ident(i) if &*i.sym == "arguments" => {
                self.bad = true;
                return;
            }
            _ => {}
        }
        e.visit_children_with(self);
    }

    fn visit_callee(&mut self, c: &Callee) {
        if matches!(c, Callee::Super(_)) {
            self.bad = true;
            return;
        }
        c.visit_children_with(self);
    }

    fn visit_member_prop(&mut self, p: &MemberProp) {
        if matches!(p, MemberProp::PrivateName(_)) {
            self.bad = true;
            return;
        }
        p.visit_children_with(self);
    }
}

/// Only names with a stable spelling; a computed key has none to build a symbol
/// from, so those members stay on the existing path.
fn simple_prop_name(key: &PropName) -> Option<String> {
    match key {
        PropName::Ident(i) => Some(i.sym.to_string()),
        PropName::Str(s) => Some(s.value.to_string_lossy().to_string()),
        _ => None,
    }
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn ident(name: &str) -> Ident {
    Ident {
        span: sp(),
        ctxt: Default::default(),
        sym: name.into(),
        optional: false,
    }
}

fn ident_expr(name: &str) -> Expr {
    Expr::Ident(ident(name))
}

fn arg(e: Expr) -> ExprOrSpread {
    ExprOrSpread {
        spread: None,
        expr: Box::new(e),
    }
}
