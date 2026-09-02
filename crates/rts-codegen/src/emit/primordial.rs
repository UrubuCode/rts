//! Whether a global name still means what the language says it means.
//!
//! # Why this exists, and why V8 cannot have it
//!
//! `Math.sqrt(x)` is a square root — one machine instruction — unless the
//! program moved `Math`, replaced `sqrt` on it, or reached either through
//! `globalThis` or `eval`. V8 answers that by ASSUMING and installing a
//! deoptimisation point: guess wrong, unwind to the interpreter, try again.
//!
//! This engine has no deoptimisation and is not growing one. A wrong guess here
//! is not slower code, it is a wrong answer. So the question is answered by
//! PROOF, and the proof is available for one reason: the whole tree is compiled
//! before anything runs. What a page does is a fact here, not a probability.
//!
//! # What is proved, and what is left to the scope
//!
//! This walk answers about MUTATION — every write that could make the name
//! refer to something else. Shadowing is not its question: whether `Math` names
//! a local is what the scope at the call site already knows exactly, and asking
//! twice is how two answers come to disagree.
//!
//! `eval` and `globalThis` end the walk rather than being analysed. An indirect
//! write is precisely the case an analysis gets wrong, and refusing costs one
//! call in a program that contains either.
//!
//! The rule this reverses was recorded on 2026-08-05 as "Math never becomes a
//! lowering", for the deoptimisation reason above. The repository's owner
//! reversed it on 2026-08-13 on the condition that the proof be whole-program.

use crate::Name;
use crate::syntax::{AssignTarget, Expr, ExprKind, Stmt, StmtKind};

use super::capture::{Child, StmtChild, walk_expr, walk_stmt};

/// Whether nothing in `body` writes to `name`, to a member of it, or brings in
/// a spelling that could reach it indirectly.
pub(super) fn untouched(body: &[Stmt], name: Name, eval: Name, global_this: Name) -> bool {
    let mut walk = Disturbance {
        name,
        eval,
        global_this,
        found: false,
    };
    for statement in body {
        walk.statement(statement);
    }
    !walk.found
}

/// The walk, carrying the three names that end it.
struct Disturbance {
    name: Name,
    eval: Name,
    global_this: Name,
    found: bool,
}

impl Disturbance {
    fn statement(&mut self, statement: &Stmt) {
        if self.found {
            return;
        }
        // `for (Math of xs)` and `for (Math.sqrt of fs)` write once per pass,
        // and `walk_stmt`'s own documentation says it is SILENT about a
        // for-each target — so the walk below cannot see them and this is the
        // one place left to ask. `for (const x of xs)` introduces a binding
        // instead and writes nothing, which is why only the assigning form is
        // asked about.
        if let StmtKind::ForEach {
            target: crate::syntax::ForEachTarget::Assign(pattern),
            ..
        } = &statement.kind
            && self.pattern_names_it(pattern)
        {
            self.found = true;
            return;
        }
        walk_stmt(statement, &mut |child| match child {
            StmtChild::Stmt(inner) => self.statement(inner),
            StmtChild::Expr(expr) => self.expression(expr),
            StmtChild::Binding(binding) => {
                if let Some(value) = &binding.value {
                    self.expression(value);
                }
            }
            StmtChild::Catch(catch) => {
                for inner in &catch.body {
                    self.statement(inner);
                }
            }
            StmtChild::Function(function) => self.body(function),
            StmtChild::Class(class) => self.members(class),
        });
    }

    fn expression(&mut self, expr: &Expr) {
        if self.found {
            return;
        }
        match &expr.kind {
            // The two spellings that end the question rather than answer it.
            ExprKind::Ident(seen) if *seen == self.eval || *seen == self.global_this => {
                self.found = true;
                return;
            }
            // `Math = {}`, `Math.sqrt = f`, `Math.sqrt ||= f`, `Math["sqrt"] = f`
            // — every form, because what matters is the target and not the
            // operator.
            ExprKind::Assign {
                target: AssignTarget::Place(place),
                ..
            } => {
                if self.names_it(place) {
                    self.found = true;
                    return;
                }
            }
            // `[Math] = xs`, `({ sqrt: Math.sqrt } = o)`, `[Math.sqrt] = fs` —
            // a DESTRUCTURING assignment writes every place its pattern names,
            // and the arm above sees none of them because they are a `Pattern`
            // rather than a `Place`.
            //
            // It was a silent wrong answer in one file, not a module hazard:
            //
            //     function zf(x) { return x + 1; }
            //     [zf] = [(x) => x + 100];
            //     console.log(zf(1));        // 2 here, 101 in node, exit 0
            //
            // — `inline::candidates` asks this same question about `zf`, saw no
            // write, and spliced the original body at the call.
            ExprKind::Assign {
                target: AssignTarget::Pattern(pattern),
                ..
            } => {
                if self.pattern_names_it(pattern) {
                    self.found = true;
                    return;
                }
            }
            // `Math.sqrt++` is a write in the same sense, and `delete Math.sqrt`
            // leaves the read answering undefined.
            ExprKind::Update { target, .. } => {
                if self.names_it(target) {
                    self.found = true;
                    return;
                }
            }
            ExprKind::Unary { operand, .. } => {
                if self.names_it(operand) {
                    self.found = true;
                    return;
                }
            }
            _ => {}
        }
        walk_expr(expr, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner),
            Child::Function(function) => self.body(function),
            Child::Class(class) => self.members(class),
        });
    }

    /// Whether a place expression is the name itself or a member of it.
    fn names_it(&self, place: &Expr) -> bool {
        match &bare(place).kind {
            ExprKind::Ident(seen) => *seen == self.name,
            ExprKind::Member { object, .. } | ExprKind::Index { object, .. } => {
                matches!(&bare(object).kind, ExprKind::Ident(seen) if *seen == self.name)
            }
            _ => false,
        }
    }

    /// Whether a destructuring pattern writes the name, at any depth.
    ///
    /// A `Name` leaf is the name itself; a `Target` leaf is any place
    /// expression, which is exactly what [`Self::names_it`] already decides. So
    /// the two questions share one answer rather than having two chances to
    /// disagree about what a write is.
    ///
    /// A DEFAULT inside the pattern is ordinary code and is walked as such —
    /// `[a = (Math.sqrt = f)] = xs` writes through an expression, not through a
    /// leaf.
    fn pattern_names_it(&mut self, pattern: &crate::syntax::Pattern) -> bool {
        use crate::syntax::Pattern;
        match pattern {
            Pattern::Name(seen) => *seen == self.name,
            Pattern::Target(place) => self.names_it(place),
            Pattern::Object(object) => {
                for property in &object.properties {
                    if let crate::syntax::PropertyKey::Computed(key) = &property.key {
                        self.expression(key);
                    }
                    if let Some(default) = &property.value.default {
                        self.expression(default);
                    }
                    if self.pattern_names_it(&property.value.pattern) {
                        return true;
                    }
                }
                object
                    .rest
                    .as_ref()
                    .is_some_and(|rest| self.pattern_names_it(rest))
            }
            Pattern::Array(array) => {
                for element in array.elements.iter().flatten() {
                    if let Some(default) = &element.default {
                        self.expression(default);
                    }
                    if self.pattern_names_it(&element.pattern) {
                        return true;
                    }
                }
                array
                    .rest
                    .as_ref()
                    .is_some_and(|rest| self.pattern_names_it(rest))
            }
        }
    }

    /// A nested function's own statements, whatever body form it has.
    fn body(&mut self, function: &crate::syntax::Function) {
        for parameter in &function.parameters {
            if let Some(default) = &parameter.default {
                self.expression(default);
            }
        }
        match &function.body {
            crate::syntax::FunctionBody::Block(statements) => {
                for statement in statements {
                    self.statement(statement);
                }
            }
            crate::syntax::FunctionBody::Expression(expr) => self.expression(expr),
        }
    }

    /// A class's initialisers and methods, which are ordinary nested code.
    fn members(&mut self, class: &crate::syntax::Class) {
        if let Some(heritage) = &class.heritage {
            self.expression(heritage);
        }
        for element in &class.body {
            walk_expr_of_element(element, self);
        }
    }
}

/// One class element’s nested code.
///
/// Split out because a class element is a small sum type and matching it inside
/// the walk above would put two levels of `match` in one function for no gain.
fn walk_expr_of_element(element: &crate::syntax::ClassElement, walk: &mut Disturbance) {
    match element {
        crate::syntax::ClassElement::Method(method) => walk.body(&method.function),
        crate::syntax::ClassElement::Field(field) => {
            if let Some(value) = &field.value {
                walk.expression(value);
            }
        }
        crate::syntax::ClassElement::StaticBlock(statements) => {
            for statement in statements {
                walk.statement(statement);
            }
        }
    }
}

/// The expression under any TypeScript wrappers.
///
/// `x as T` parses to `ExprKind::Asserted`, and `x!` to the same shape, so
/// `(Math as any).sqrt = f` is a `Member` whose OBJECT is an assertion rather
/// than the identifier. Comparing the object against `ExprKind::Ident` therefore
/// answered no, and the write was invisible:
///
/// ```text
/// Math.sqrt = () => 42;           console.log(Math.sqrt(16));   // 42, correct
/// (Math as any).sqrt = () => 42;  console.log(Math.sqrt(16));   // 4,  WRONG
/// ```
///
/// Both are the same program. The second is how the write is spelled in
/// TypeScript, which is the language this engine compiles — so the shape that
/// escaped the proof is the shape a real program uses.
///
/// A claim carries no value and evaluates nothing, so stepping through it can
/// never skip an effect. It is written as a loop because `x as unknown as T` is
/// two of them, and as a function rather than an arm so that the two questions
/// this walk asks — is this place the name, is this object the name — cannot
/// disagree about it.
fn bare(expr: &Expr) -> &Expr {
    let mut seen = expr;
    while let ExprKind::Asserted { value, .. } = &seen.kind {
        seen = value;
    }
    seen
}
