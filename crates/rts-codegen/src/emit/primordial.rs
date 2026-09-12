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
///
/// One walk for one name. A caller with MANY names to ask about the same body
/// — `inline::candidates`, once per helper the body declares — asks
/// [`disturbed`] once instead and queries it, because this walk over a
/// seven-thousand-node body, a thousand times, was the largest single term of
/// a quadratic compile (measured 2026-09-11 on a synthetic body of N helpers:
/// 3 s at 374 KB, 10 s at 748 KB, 37 s at 1.5 MB; the 4.5 MB WhatsApp Web
/// bundle never finished). Both forms read the same walk, so they cannot
/// disagree about what a write is.
pub(super) fn untouched(body: &[Stmt], name: Name, eval: Name, global_this: Name) -> bool {
    disturbed(body, eval, global_this).untouched(name)
}

/// Every name `body` writes to, directly or through a member — and whether it
/// reaches for `eval` or `globalThis` at all, which disturbs every name at once.
pub(super) struct Disturbed {
    names: std::collections::BTreeSet<Name>,
    indirect: bool,
}

impl Disturbed {
    /// The answer [`untouched`] gives for `name`, from the walk already done.
    pub(super) fn untouched(&self, name: Name) -> bool {
        !self.indirect && !self.names.contains(&name)
    }
}

/// The walk behind [`untouched`], done once for every name at the same time.
pub(super) fn disturbed(body: &[Stmt], eval: Name, global_this: Name) -> Disturbed {
    let mut walk = Disturbance {
        eval,
        global_this,
        names: std::collections::BTreeSet::new(),
        indirect: false,
    };
    for statement in body {
        walk.statement(statement);
    }
    Disturbed {
        names: walk.names,
        indirect: walk.indirect,
    }
}

/// The walk: collects every written name, and stops at the first `eval` or
/// `globalThis`, after which no name can be proved anything.
struct Disturbance {
    eval: Name,
    global_this: Name,
    names: std::collections::BTreeSet<Name>,
    indirect: bool,
}

impl Disturbance {
    fn statement(&mut self, statement: &Stmt) {
        if self.indirect {
            return;
        }
        if let StmtKind::ForEach {
            target: crate::syntax::ForEachTarget::Assign(pattern),
            ..
        } = &statement.kind
        {
            self.pattern_writes(pattern);
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
        if self.indirect {
            return;
        }
        match &expr.kind {
            ExprKind::Ident(seen) if *seen == self.eval || *seen == self.global_this => {
                self.indirect = true;
                return;
            }
            ExprKind::Assign {
                target: AssignTarget::Place(place),
                ..
            } => self.place_writes(place),
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
            } => self.pattern_writes(pattern),
            // `Math.sqrt++` is a write in the same sense, and `delete Math.sqrt`
            // leaves the read answering undefined.
            ExprKind::Update { target, .. } => self.place_writes(target),
            ExprKind::Unary { operand, .. } => self.place_writes(operand),
            _ => {}
        }
        walk_expr(expr, &mut |child| match child {
            Child::Expr(inner) => self.expression(inner),
            Child::Function(function) => self.body(function),
            Child::Class(class) => self.members(class),
        });
    }

    /// Records the name a place expression writes: the name itself, or the
    /// object whose member it is.
    fn place_writes(&mut self, place: &Expr) {
        let object = match &bare(place).kind {
            ExprKind::Ident(seen) => Some(*seen),
            ExprKind::Member { object, .. } | ExprKind::Index { object, .. } => {
                match &bare(object).kind {
                    ExprKind::Ident(seen) => Some(*seen),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some(name) = object {
            self.names.insert(name);
        }
    }

    /// Records every name a destructuring pattern writes, at any depth.
    ///
    /// A `Name` leaf is the name itself; a `Target` leaf is any place
    /// expression, which is exactly what [`Self::place_writes`] already decides.
    /// So the two questions share one answer rather than having two chances to
    /// disagree about what a write is.
    ///
    /// A DEFAULT inside the pattern is ordinary code and is walked as such —
    /// `[a = (Math.sqrt = f)] = xs` writes through an expression, not through a
    /// leaf.
    fn pattern_writes(&mut self, pattern: &crate::syntax::Pattern) {
        use crate::syntax::Pattern;
        match pattern {
            Pattern::Name(seen) => {
                self.names.insert(*seen);
            }
            Pattern::Target(place) => self.place_writes(place),
            Pattern::Object(object) => {
                for property in &object.properties {
                    if let crate::syntax::PropertyKey::Computed(key) = &property.key {
                        self.expression(key);
                    }
                    if let Some(default) = &property.value.default {
                        self.expression(default);
                    }
                    self.pattern_writes(&property.value.pattern);
                }
                if let Some(rest) = &object.rest {
                    self.pattern_writes(rest);
                }
            }
            Pattern::Array(array) => {
                for element in array.elements.iter().flatten() {
                    if let Some(default) = &element.default {
                        self.expression(default);
                    }
                    self.pattern_writes(&element.pattern);
                }
                if let Some(rest) = &array.rest {
                    self.pattern_writes(rest);
                }
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
