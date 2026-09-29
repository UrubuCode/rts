//! A class as a LAYOUT: `new C(…)` that never has to construct anything.
//!
//! # What this buys, measured before it was written
//!
//! `const o = new F1(i); s += o.a` costs 80 ns where the same thing written as
//! `const o = { a: i }` costs 3.9 (release, 2026-09-29): the literal never
//! becomes an object — `escape` turns it into one local per property — and the
//! instance is allocated, constructed through the runtime and collected. The
//! class says nothing the literal does not. Its fields, their order and what is
//! written to each are all in the declaration, which is what a language with
//! layouts reads a class AS.
//!
//! # What is done, and what deliberately is not
//!
//! A declaration `const o = new C(a, b)` is REWRITTEN, in the body about to be
//! emitted, to the object literal `C`'s constructor would have filled — and
//! only where `escape` then proves that literal never has to exist. So this
//! file decides two things and `escape` decides the third:
//!
//! - which classes have a layout ([`layouts`]);
//! - which sites may use it ([`Layout::literal`]);
//! - whether the object is ever seen, which is `escape::analyse`'s one
//!   statement of that rule, asked of the rewritten body.
//!
//! An instance that ESCAPES is left exactly as written. The literal has no
//! prototype, so handing it to anything that can look — a call, a store, a
//! method, `instanceof` — would hand over a different object; every such use
//! is a use `escape` refuses, which is what makes the rewrite invisible.
//!
//! # Which class has a layout
//!
//! Every clause is a way the constructor could do something a literal does not:
//!
//! - **declared once in the program and never assigned** — `inline`'s pair of
//!   whole-program facts, for the same reason: the site names the class by its
//!   spelling;
//! - **no `extends`**: the parent's constructor runs first and is another
//!   class's layout — the next step, not this one;
//! - **no accessor, no private or computed member**: `this.k = v` reaches a
//!   setter where a literal defines a property;
//! - **`Object` is only ever a base** (`Primordials::object`), so nothing
//!   installed a setter on `Object.prototype` by a road this can see;
//! - **a field initialiser names nothing**: it is evaluated per instance with
//!   `this` bound, and a literal has neither that scope nor that receiver;
//! - **the constructor is a list of `this.k = value`**, each value a parameter
//!   or something that names nothing. No `return`, no call, no read of `this`.
//!
//! # Which site may use it
//!
//! The constructor's parameters are evaluated BEFORE its body; a literal
//! evaluates each value where it is written. So an argument is substituted only
//! when moving it cannot be seen: it is a name or a literal, or every parameter
//! is written to a field bare, once, in parameter order.
//!
//! # The read of the class that stays
//!
//! `new C()` before `C` holds its class raises, and a function declared above
//! the class and called before it is how a program gets there. The rewrite
//! keeps that: the statement is preceded by `C.prototype;`, which raises on
//! exactly the values `new` raises on here — a binding not yet written reads
//! `undefined` in this engine, and a member of `undefined` is a `TypeError` as
//! constructing it is. A bare read of `C` was written first and is not enough:
//! it answers `undefined` and carries on, so a construction that used to raise
//! built an object instead.
//!
//! Refusing every class not declared above all the code that runs was the
//! alternative, and it refuses most programs: a constant declared above a class
//! is code that runs.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::inline::Declarations;
use super::primordial::Disturbed;
use crate::names::{Name, Names};
use crate::syntax::{
    AssignOp, AssignTarget, Class, ClassElement, ClassKey, Expr, ExprKind, FunctionBody, Literal,
    MethodKind, Pattern, Property, PropertyKey, Spreadable, Stmt, StmtKind,
};
use crate::values::Singleton;

/// What one field is given.
#[derive(Clone)]
enum Given {
    /// The parameter at this position, bare.
    Parameter(usize),
    /// An expression that names nothing.
    Closed(Expr),
}

/// A class whose instances are a list of fields and nothing else.
pub(super) struct Layout {
    parameters: usize,
    fields: Vec<(Name, Given)>,
    /// Whether every parameter is written to a field once, in order.
    in_order: bool,
}

/// Every class in `body` that has a layout, by the name a site constructs it
/// under. Only declarations at the top of `body`: a class declared inside a
/// function is a different class per call.
pub(super) fn layouts(
    body: &[Stmt],
    declared: &Declarations,
    writes: &Disturbed,
    names: &Names,
) -> BTreeMap<Name, Rc<Layout>> {
    let mut found = BTreeMap::new();
    for statement in body {
        let StmtKind::Class(class) = &statement.kind else {
            continue;
        };
        let Some(name) = class.name else {
            continue;
        };
        if declared.count(name) != 1 || !writes.untouched(name) {
            continue;
        }
        if let Some(layout) = layout_of(class, names) {
            found.insert(name, Rc::new(layout));
        }
    }
    found
}

fn layout_of(class: &Class, names: &Names) -> Option<Layout> {
    if class.heritage.is_some() {
        return None;
    }
    let mut fields: Vec<(Name, Given)> = Vec::new();
    let mut constructor = None;
    for element in &class.body {
        match element {
            ClassElement::StaticBlock(_) => {}
            ClassElement::Method(method) => {
                if method.kind != MethodKind::Normal {
                    return None;
                }
                let ClassKey::Public(PropertyKey::Named(_)) = &method.key else {
                    return None;
                };
                if method.is_constructor(names) {
                    constructor = Some(&method.function);
                }
            }
            ClassElement::Field(field) => {
                let ClassKey::Public(PropertyKey::Named(key)) = &field.key else {
                    return None;
                };
                if field.is_static {
                    continue;
                }
                let value = match &field.value {
                    Some(value) if closed(value) => value.clone(),
                    Some(_) => return None,
                    None => undefined(class.at),
                };
                write(&mut fields, *key, Given::Closed(value), true)?;
            }
        }
    }

    let mut parameters = Vec::new();
    if let Some(function) = constructor {
        if function.is_async || function.is_generator || function.rest_parameter.is_some() {
            return None;
        }
        for parameter in &function.parameters {
            let Pattern::Name(name) = &parameter.target else {
                return None;
            };
            if parameter.default.is_some() || parameters.contains(name) {
                return None;
            }
            parameters.push(*name);
        }
        let FunctionBody::Block(body) = &function.body else {
            return None;
        };
        for statement in body {
            let (key, value) = field_write(statement)?;
            let given = match &value.kind {
                ExprKind::Ident(name) => {
                    Given::Parameter(parameters.iter().position(|held| held == name)?)
                }
                _ if closed(value) => Given::Closed(value.clone()),
                _ => return None,
            };
            write(&mut fields, key, given, false)?;
        }
    }

    let written: Vec<usize> = fields
        .iter()
        .filter_map(|(_, given)| match given {
            Given::Parameter(at) => Some(*at),
            Given::Closed(_) => None,
        })
        .collect();
    let in_order = written.iter().copied().eq(0..parameters.len());
    Some(Layout {
        parameters: parameters.len(),
        fields,
        in_order,
    })
}

/// Records one write. An initialiser may be written over by the constructor —
/// it names nothing, so dropping it cannot be seen — and nothing else may be
/// written twice.
fn write(fields: &mut Vec<(Name, Given)>, key: Name, given: Given, initialiser: bool) -> Option<()> {
    match fields.iter_mut().find(|(held, _)| *held == key) {
        None => fields.push((key, given)),
        Some(_) if initialiser => return None,
        Some((_, held)) => match held {
            Given::Closed(_) => *held = given,
            Given::Parameter(_) => return None,
        },
    }
    Some(())
}

/// `this.k = value;`, and nothing else.
fn field_write(statement: &Stmt) -> Option<(Name, &Expr)> {
    let StmtKind::Expr(Expr {
        kind:
            ExprKind::Assign {
                target: AssignTarget::Place(place),
                value,
                op: AssignOp::Plain,
            },
        ..
    }) = &statement.kind
    else {
        return None;
    };
    let ExprKind::Member {
        object,
        property,
        optional: false,
    } = &place.kind
    else {
        return None;
    };
    matches!(object.kind, ExprKind::This).then_some((*property, &**value))
}

/// Whether `expr` names nothing and runs nothing: what it answers is the same
/// wherever and whenever it is evaluated, and evaluating it cannot be seen.
///
/// An allow-list. Arithmetic is on it only because both operands are closed,
/// which makes them primitives or fresh literals — nothing with a `valueOf` a
/// program wrote.
fn closed(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(Literal::Regex { .. }) => false,
        ExprKind::Literal(_) => true,
        ExprKind::Unary { op, operand } => {
            op.reads_a_value() && primitive(operand)
        }
        ExprKind::Binary { left, right, .. } => primitive(left) && primitive(right),
        ExprKind::Array { elements } => elements.iter().all(|element| match element {
            Some(Spreadable::Single(value)) => closed(value),
            _ => false,
        }),
        ExprKind::Object { properties } => properties.iter().all(|property| match property {
            Property::Value {
                key: PropertyKey::Named(_),
                value,
                ..
            } => closed(value),
            _ => false,
        }),
        _ => false,
    }
}

/// A closed expression that is certainly a primitive.
fn primitive(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(Literal::Regex { .. }) => false,
        ExprKind::Literal(_) => true,
        ExprKind::Unary { op, operand } => op.reads_a_value() && primitive(operand),
        ExprKind::Binary { left, right, .. } => primitive(left) && primitive(right),
        _ => false,
    }
}

fn undefined(at: rts_cranelift::fault::Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
        at,
    }
}

/// An argument that may be evaluated anywhere, any number of times.
fn inert(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Ident(_) | ExprKind::Literal(Literal::Number(_) | Literal::String(_) | Literal::Boolean(_) | Literal::Singleton(_))
    )
}

impl Layout {
    /// The literal `new C(arguments)` fills, or `None` where moving an argument
    /// could be seen.
    fn literal(&self, arguments: &[Spreadable], at: rts_cranelift::fault::Position) -> Option<Expr> {
        let mut written = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let Spreadable::Single(value) = argument else {
                return None;
            };
            written.push(value);
        }
        let all_inert = written.iter().all(|value| inert(value));
        if !all_inert && !(self.in_order && written.len() == self.parameters) {
            return None;
        }
        if self.fields.is_empty() {
            return None;
        }
        let properties = self
            .fields
            .iter()
            .map(|(key, given)| Property::Value {
                key: PropertyKey::Named(*key),
                value: match given {
                    Given::Closed(value) => value.clone(),
                    Given::Parameter(position) => match written.get(*position) {
                        Some(value) => (*value).clone(),
                        None => undefined(at),
                    },
                },
                shorthand: false,
            })
            .collect();
        Some(Expr {
            kind: ExprKind::Object { properties },
            at,
        })
    }
}

/// `body` with every `const o = new C(…)` it may rewrite rewritten, or `None`
/// where there is nothing to rewrite.
///
/// Two passes, because the rewrite is only legal where the object is never
/// seen and only the rewritten body can be asked: every site is rewritten,
/// `escape` is asked, and the sites it refused are put back as written.
pub(super) fn rewritten(
    layouts: &BTreeMap<Name, Rc<Layout>>,
    body: &[Stmt],
    parameters: &[Name],
    prototype: Name,
) -> Option<Vec<Stmt>> {
    if layouts.is_empty() {
        return None;
    }
    let mut tried = body.to_vec();
    let mut names = BTreeSet::new();
    rewrite_list(&mut tried, layouts, None, &mut names, prototype);
    if names.is_empty() {
        return None;
    }
    // A name a nested function mentions is one that function can see whole.
    let candidates: Vec<Name> = names.iter().copied().collect();
    let captured = super::capture::captured(&tried, &candidates, &BTreeSet::new());
    let flattened = super::escape::analyse(&tried, parameters, &captured);
    let kept: BTreeSet<Name> = names
        .iter()
        .copied()
        .filter(|name| flattened.properties(*name).is_some())
        .collect();
    if kept.is_empty() {
        return None;
    }
    if kept.len() == names.len() {
        return Some(tried);
    }
    let mut settled = body.to_vec();
    rewrite_list(&mut settled, layouts, Some(&kept), &mut BTreeSet::new(), prototype);
    Some(settled)
}

/// Rewrites the declarations of one statement list, and of the lists inside it.
/// A nested function or class is not entered: it is rewritten when IT is
/// emitted. `with` is not entered either — a name inside one may be a property.
fn rewrite_list(
    list: &mut Vec<Stmt>,
    layouts: &BTreeMap<Name, Rc<Layout>>,
    only: Option<&BTreeSet<Name>>,
    names: &mut BTreeSet<Name>,
    prototype: Name,
) {
    let mut at = 0;
    while at < list.len() {
        let mut checks = Vec::new();
        match &mut list[at].kind {
            StmtKind::Declare { kind, bindings } if kind.is_block_scoped() => {
                for binding in bindings.iter_mut() {
                    let Pattern::Name(name) = &binding.target else {
                        continue;
                    };
                    if only.is_some_and(|kept| !kept.contains(name)) {
                        continue;
                    }
                    let Some(Expr {
                        kind: ExprKind::New { callee, arguments },
                        at: position,
                    }) = &binding.value
                    else {
                        continue;
                    };
                    let ExprKind::Ident(class) = &callee.kind else {
                        continue;
                    };
                    let Some(literal) = layouts
                        .get(class)
                        .and_then(|layout| layout.literal(arguments, *position))
                    else {
                        continue;
                    };
                    checks.push(Expr {
                        kind: ExprKind::Member {
                            object: callee.clone(),
                            property: prototype,
                            optional: false,
                        },
                        at: *position,
                    });
                    names.insert(*name);
                    binding.value = Some(literal);
                }
            }
            StmtKind::Block(inner) => rewrite_list(inner, layouts, only, names, prototype),
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                rewrite_boxed(then_branch, layouts, only, names, prototype);
                if let Some(other) = else_branch {
                    rewrite_boxed(other, layouts, only, names, prototype);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. }
            | StmtKind::Labelled { body, .. } => rewrite_boxed(body, layouts, only, names, prototype),
            StmtKind::Switch { clauses, .. } => {
                for clause in clauses {
                    rewrite_list(&mut clause.body, layouts, only, names, prototype);
                }
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                rewrite_list(body, layouts, only, names, prototype);
                if let Some(catch) = catch {
                    rewrite_list(&mut catch.body, layouts, only, names, prototype);
                }
                if let Some(finally) = finally {
                    rewrite_list(finally, layouts, only, names, prototype);
                }
            }
            _ => {}
        }
        // What `new` would have asked of the class, before the statement and
        // in the order the bindings were written.
        let position = list[at].at;
        let count = checks.len();
        for (offset, check) in checks.into_iter().enumerate() {
            list.insert(
                at + offset,
                Stmt {
                    kind: StmtKind::Expr(check),
                    at: position,
                },
            );
        }
        at += count + 1;
    }
}

/// The same, for a statement that is a body: only a block holds a list.
fn rewrite_boxed(
    statement: &mut Stmt,
    layouts: &BTreeMap<Name, Rc<Layout>>,
    only: Option<&BTreeSet<Name>>,
    names: &mut BTreeSet<Name>,
    prototype: Name,
) {
    if let StmtKind::Block(inner) = &mut statement.kind {
        rewrite_list(inner, layouts, only, names, prototype);
    }
}
