//! A method with `const` locals, and a method that calls another method of
//! its class — the two shapes `method_of` refused, and what admits them.
//!
//! # Why both are substitutions and not statements
//!
//! A method call is rewritten to ONE expression over the fields (see the
//! parent module), so whatever a method's body holds has to become part of
//! that expression. A `const` local becomes its initialiser, written where the
//! name was; a call to `this.m(…)` becomes what `m` would have been rewritten
//! to at that site. Neither needs a new form of expression, and neither is
//! visible: the body only READS (`reads_only`), so evaluating a pure
//! expression in one place rather than another cannot be observed — with the
//! two exceptions [`locals_admitted`] refuses.
//!
//! # What is refused, and why each is a refusal rather than a rule
//!
//! - a local whose initialiser reads a field, in a method that WRITES a field:
//!   `const a = this.x; this.x = 5; return a` would read the new value;
//! - a local used more than TWICE whose initialiser holds a call: substituting
//!   it runs it once per use, and three square roots are where the copies cost
//!   what they save. Operators over fields and parameters are duplicated
//!   freely — a nanosecond each against the fifty the materialised instance
//!   costs. [`duplicable`] is the rule;
//! - an argument to an inner method call that is not a name or a literal,
//!   where the callee writes a field, or reads the parameter more often than
//!   [`duplicable`] allows — the same two hazards, seen from the callee's side;
//! - a chain of calls deeper than [`DEPTH`]: a method calling itself is a
//!   recursion, and a rewrite has no way to stop.
//!
//! Every refusal leaves the call as written, which is what happened before
//! this module existed: the instance is materialised and the method is called.

use super::*;

/// How deep a method may call another method of the class before the
/// expansion gives up. Real code is two or three; a loop is unbounded.
const DEPTH: usize = 8;

/// `expr` with every `const` local spelled as its initialiser.
///
/// Total over the shapes `reads_only` admits; anything else is cloned as
/// written, and the caller's `reads_only` then refuses it.
pub(super) fn with_locals(expr: &Expr, locals: &[(Name, Expr)]) -> Expr {
    if locals.is_empty() {
        return expr.clone();
    }
    let again = |inner: &Expr| Box::new(with_locals(inner, locals));
    let kind = match &expr.kind {
        ExprKind::Ident(name) => {
            return match locals.iter().find(|(held, _)| held == name) {
                Some((_, value)) => value.clone(),
                None => expr.clone(),
            };
        }
        ExprKind::Call {
            callee,
            arguments,
            optional,
        } => ExprKind::Call {
            callee: again(callee),
            arguments: arguments
                .iter()
                .map(|argument| match argument {
                    Spreadable::Single(value) => Spreadable::Single(with_locals(value, locals)),
                    Spreadable::Spread(value) => Spreadable::Spread(value.clone()),
                })
                .collect(),
            optional: *optional,
        },
        ExprKind::Assign {
            target: AssignTarget::Place(place),
            value,
            op,
        } => ExprKind::Assign {
            target: AssignTarget::Place(again(place)),
            value: again(value),
            op: *op,
        },
        ExprKind::Unary { op, operand } => ExprKind::Unary {
            op: *op,
            operand: again(operand),
        },
        ExprKind::Binary { op, left, right } => ExprKind::Binary {
            op: *op,
            left: again(left),
            right: again(right),
        },
        ExprKind::Logical { op, left, right } => ExprKind::Logical {
            op: *op,
            left: again(left),
            right: again(right),
        },
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::Conditional {
            condition: again(condition),
            then_branch: again(then_branch),
            else_branch: again(else_branch),
        },
        other => other.clone(),
    };
    Expr { kind, at: expr.at }
}

/// Whether the locals a method declares may be substituted into it.
///
/// `later` is every statement after the first declaration, as written, and
/// `answer` the return; the counts are taken over the text the program wrote,
/// so a local used inside another local's initialiser counts where it is
/// written and not where the other is used.
pub(super) fn locals_admitted(
    locals: &[(Name, Expr)],
    writes_a_field: bool,
    later: &[Stmt],
    answer: Option<&Expr>,
) -> bool {
    locals.iter().all(|(name, init)| {
        if writes_a_field && mentions_this(init) {
            return false;
        }
        let uses = later.iter().map(|statement| uses_in_statement(*name, statement)).sum::<usize>()
            + answer.map_or(0, |value| uses(*name, value));
        duplicable(init, uses)
    })
}

/// Whether an expression evaluated once as written may be written `uses`
/// times instead.
///
/// Anything without a call in it, freely: fields, parameters and operators
/// over them are a nanosecond each where the materialised instance they save
/// is fifty. An expression holding a call — `Math.sqrt(…)`, another method —
/// at most TWICE: `const d = this.len(); return this.x / d + this.y / d` is
/// how a normalisation is written, and two square roots (about 10 ns) are
/// still well under the instance; a third use is where the copies start to
/// cost what they save, and that method is left as a call.
fn duplicable(expr: &Expr, uses: usize) -> bool {
    uses <= 1 || trivial(expr) || uses <= 2
}

/// Whether no call is in it.
fn trivial(expr: &Expr) -> bool {
    let mut calls = matches!(expr.kind, ExprKind::Call { .. });
    walk_expr(expr, &mut |child| {
        if let Child::Expr(inner) = child {
            calls |= !trivial(inner);
        }
    });
    !calls
}

fn mentions_this(expr: &Expr) -> bool {
    let mut found = matches!(expr.kind, ExprKind::This);
    walk_expr(expr, &mut |child| {
        if let Child::Expr(inner) = child {
            found |= mentions_this(inner);
        }
    });
    found
}

/// How many times `name` is read in `expr`.
pub(super) fn uses(name: Name, expr: &Expr) -> usize {
    let mut count = usize::from(matches!(&expr.kind, ExprKind::Ident(held) if *held == name));
    walk_expr(expr, &mut |child| {
        if let Child::Expr(inner) = child {
            count += uses(name, inner);
        }
    });
    count
}

fn uses_in_statement(name: Name, statement: &Stmt) -> usize {
    match &statement.kind {
        StmtKind::Expr(value) | StmtKind::Return(Some(value)) => uses(name, value),
        StmtKind::Declare { bindings, .. } => bindings
            .iter()
            .filter_map(|binding| binding.value.as_ref())
            .map(|value| uses(name, value))
            .sum(),
        _ => 0,
    }
}

impl Layout {
    /// What `object.name(arguments)` answers, as an expression over the fields,
    /// with every call to another method of the class expanded in turn.
    pub(super) fn call_deep(
        &self,
        object: Name,
        name: Name,
        arguments: &[Spreadable],
        depth: usize,
    ) -> Option<Expr> {
        if depth > DEPTH {
            return None;
        }
        let method = self.methods.get(&name)?;
        let mut written = Vec::with_capacity(arguments.len());
        for (at, argument) in arguments.iter().enumerate() {
            let Spreadable::Single(value) = argument else {
                return None;
            };
            // A name or a literal moves freely. Anything else is an expression
            // a method body produced — pure by `reads_only` — and may be moved
            // into a callee that does not write and reads it at most once.
            let admitted = inert(value)
                || (method.writes.is_empty()
                    && method
                        .parameters
                        .get(at)
                        .is_none_or(|parameter| {
                            method.answer.as_ref().is_none_or(|answer| duplicable(value, uses(*parameter, answer)))
                        }));
            if !admitted {
                return None;
            }
            written.push(value);
        }
        let answer = match &method.answer {
            Some(answer) => spelled(answer, method, &written, object),
            None => undefined(rts_cranelift::fault::Position::default()),
        };
        let answer = self.expanded(answer, object, depth)?;
        if method.writes.is_empty() {
            return Some(answer);
        }
        let at = answer.at;
        let mut operands = Vec::with_capacity(method.writes.len() + 1);
        for write in &method.writes {
            let write = spelled(write, method, &written, object);
            operands.push(self.expanded(write, object, depth)?);
        }
        operands.push(answer);
        Some(Expr {
            kind: ExprKind::Sequence { operands },
            at,
        })
    }

    /// `expr` with every `object.m(…)` naming a method of this class replaced
    /// by what that call answers.
    fn expanded(&self, expr: Expr, object: Name, depth: usize) -> Option<Expr> {
        let again = |inner: &Expr| self.expanded(inner.clone(), object, depth).map(Box::new);
        let at = expr.at;
        let kind = match expr.kind {
            ExprKind::Call {
                callee,
                arguments,
                optional: false,
            } => {
                let arguments: Option<Vec<_>> = arguments
                    .into_iter()
                    .map(|argument| match argument {
                        Spreadable::Single(value) => {
                            self.expanded(value, object, depth).map(Spreadable::Single)
                        }
                        Spreadable::Spread(_) => None,
                    })
                    .collect();
                let arguments = arguments?;
                if let ExprKind::Member {
                    object: held,
                    property,
                    optional: false,
                } = &callee.kind
                    && matches!(&held.kind, ExprKind::Ident(name) if *name == object)
                    && self.methods.contains_key(property)
                {
                    return self.call_deep(object, *property, &arguments, depth + 1);
                }
                ExprKind::Call {
                    callee,
                    arguments,
                    optional: false,
                }
            }
            ExprKind::Assign {
                target: AssignTarget::Place(place),
                value,
                op,
            } => ExprKind::Assign {
                target: AssignTarget::Place(again(&place)?),
                value: again(&value)?,
                op,
            },
            ExprKind::Unary { op, operand } => ExprKind::Unary {
                op,
                operand: again(&operand)?,
            },
            ExprKind::Binary { op, left, right } => ExprKind::Binary {
                op,
                left: again(&left)?,
                right: again(&right)?,
            },
            ExprKind::Logical { op, left, right } => ExprKind::Logical {
                op,
                left: again(&left)?,
                right: again(&right)?,
            },
            ExprKind::Conditional {
                condition,
                then_branch,
                else_branch,
            } => ExprKind::Conditional {
                condition: again(&condition)?,
                then_branch: again(&then_branch)?,
                else_branch: again(&else_branch)?,
            },
            other => other,
        };
        Some(Expr { kind, at })
    }
}
