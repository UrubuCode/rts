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
    let items = std::mem::take(&mut program.items);
    program.items = items
        .into_iter()
        .map(|item| match item {
            // `async function` is its own rewrite (returns a Promise) — never a ctor.
            Item::Function(f) if !f.is_async => match function_to_class(&f) {
                Some(class) => Item::Class(class),
                None => Item::Function(f),
            },
            other => other,
        })
        .collect();
    program
}

/// Build a synthetic `ClassDecl` from `f` iff its body writes to `this.<field>`.
/// `None` for an ordinary function (no `this` field writes) — left untouched.
fn function_to_class(f: &FunctionDecl) -> Option<ClassDecl> {
    let fields = discover_this_fields(&f.body);
    if fields.is_empty() {
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
    Some(ClassDecl {
        name: f.name.clone(),
        super_class: None,
        members,
        is_abstract: false,
        static_init_body: Vec::new(),
        static_init_blocks: Vec::new(),
        exported: f.exported,
        span: f.span,
    })
}

/// Collect the `this.<ident>` field names assigned in `body`, first-seen order.
fn discover_this_fields(body: &[Statement]) -> Vec<String> {
    let mut c = ThisFieldCollector {
        out: Vec::new(),
        seen: HashSet::new(),
    };
    for s in body {
        let Statement::Raw(raw) = s;
        if let Some(stmt) = &raw.stmt {
            stmt.visit_with(&mut c);
        }
    }
    c.out
}

struct ThisFieldCollector {
    out: Vec<String>,
    seen: HashSet<String>,
}

impl Visit for ThisFieldCollector {
    fn visit_assign_expr(&mut self, node: &swc_ecma_ast::AssignExpr) {
        if let Some(name) = this_member_field(&node.left) {
            if self.seen.insert(name.clone()) {
                self.out.push(name);
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
