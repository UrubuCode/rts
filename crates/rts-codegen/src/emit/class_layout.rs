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
//! - **`extends` only of a class that has a layout itself**, named bare. Then
//!   the instance is the parent's fields followed by its own, which is what a
//!   language with layouts makes of inheritance: `super(a, b)` is the parent's
//!   writes, given `a` and `b`, and it has to be the constructor's first
//!   statement because nothing may be written before it;
//! - **no accessor, no private or computed member**: `this.k = v` reaches a
//!   setter where a literal defines a property;
//! - **`Object` is only ever a base** (`Primordials::object`), so nothing
//!   installed a setter on `Object.prototype` by a road this can see;
//! - **a field initialiser names nothing**: it is evaluated per instance with
//!   `this` bound, and a literal has neither that scope nor that receiver;
//! - **the constructor is a list of `this.k = value`**, each value a parameter
//!   or something that names nothing. No `return`, no call, no read of `this`.
//!
//! - **never read as a value** (`receiver::handed_over`): `C.prototype` reached
//!   at all is how a setter or another method gets installed from outside, and
//!   a class handed to a call can have anything done to it.
//!
//! # A method called on the instance
//!
//! `o.norm()` hands `o` to `norm`, which is a use `escape` refuses — so one
//! method call made the instance exist (92 ns against 4.5). Where the method is
//! one expression over its own fields and parameters, the call is rewritten to
//! that expression with `this.k` spelled `o.k`, and what is left is field reads.
//! `receiver.rs` substitutes the same call for a receiver that is an OBJECT;
//! this is the form for one that is not going to be.
//!
//! The arguments of such a call must be names or literals: the body evaluates
//! its fields and operators in its own order, and an argument that runs code
//! would run it at a different point.
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

/// A method that is one expression over the fields and its own parameters.
struct Method {
    parameters: Vec<Name>,
    answer: Expr,
}

/// A class whose instances are a list of fields and nothing else.
pub(super) struct Layout {
    parameters: usize,
    fields: Vec<(Name, Given)>,
    methods: BTreeMap<Name, Rc<Method>>,
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
    handed_over: &BTreeSet<Name>,
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
        if declared.count(name) != 1 || !writes.untouched(name) || handed_over.contains(&name) {
            continue;
        }
        if let Some(layout) = layout_of(class, names, &found) {
            found.insert(name, Rc::new(layout));
        }
    }
    found
}

fn layout_of(
    class: &Class,
    names: &Names,
    found: &BTreeMap<Name, Rc<Layout>>,
) -> Option<Layout> {
    // The parent, which must be one of the classes already found: it is
    // declared above, or `extends` would have nothing to read.
    let parent = match &class.heritage {
        None => None,
        Some(Expr {
            kind: ExprKind::Ident(name),
            ..
        }) => Some(found.get(name)?.clone()),
        Some(_) => return None,
    };
    let mut fields: Vec<(Name, Given)> = Vec::new();
    let mut initialised: Vec<Name> = Vec::new();
    let mut constructor = None;
    let mut written_methods: Vec<(Name, &crate::syntax::Function)> = Vec::new();
    for element in &class.body {
        match element {
            ClassElement::StaticBlock(_) => {}
            ClassElement::Method(method) => {
                if method.kind != MethodKind::Normal {
                    return None;
                }
                let ClassKey::Public(PropertyKey::Named(key)) = &method.key else {
                    return None;
                };
                if method.is_constructor(names) {
                    constructor = Some(&method.function);
                } else if !method.is_static {
                    written_methods.push((*key, &method.function));
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
                if initialised.contains(key) {
                    return None;
                }
                initialised.push(*key);
                fields.push((*key, Given::Closed(value)));
            }
        }
    }
    // The initialisers were gathered first because the body is read once; they
    // RUN after the parent's writes, so they are put behind them below.
    let own = std::mem::take(&mut fields);

    let mut parameters = Vec::new();
    let mut written_body: &[Stmt] = &[];
    // How many arguments the constructor names: its own parameters, or the
    // parent's where it is the implicit one.
    let mut arity = 0;
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
        written_body = body;
        arity = parameters.len();
    }
    // What a constructor's value is: a parameter bare, or something closed.
    let given_by = |value: &Expr, parameters: &[Name]| match &value.kind {
        ExprKind::Ident(name) => parameters
            .iter()
            .position(|held| held == name)
            .map(Given::Parameter),
        _ if closed(value) => Some(Given::Closed(value.clone())),
        _ => None,
    };

    if let Some(parent) = &parent {
        // What the parent is handed, by position: `super(…)`'s arguments, or
        // this class's own parameters where no constructor is written — the
        // implicit one passes every argument on.
        let handed: Vec<Given> = match constructor {
            None => {
                arity = parent.parameters;
                (0..parent.parameters).map(Given::Parameter).collect()
            }
            Some(_) => {
                let (first, rest) = written_body.split_first()?;
                let StmtKind::Expr(Expr {
                    kind: ExprKind::SuperCall { arguments },
                    ..
                }) = &first.kind
                else {
                    return None;
                };
                written_body = rest;
                let mut handed = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let Spreadable::Single(value) = argument else {
                        return None;
                    };
                    handed.push(given_by(value, &parameters)?);
                }
                handed
            }
        };
        for (key, given) in &parent.fields {
            let given = match given {
                Given::Closed(value) => Given::Closed(value.clone()),
                Given::Parameter(at) => match handed.get(*at) {
                    Some(given) => given.clone(),
                    None => Given::Closed(undefined(class.at)),
                },
            };
            fields.push((*key, given));
        }
    }
    for (key, given) in own {
        write(&mut fields, key, given)?;
    }
    for statement in written_body {
        let (key, value) = field_write(statement)?;
        write(&mut fields, key, given_by(value, &parameters)?)?;
    }

    let written: Vec<usize> = fields
        .iter()
        .filter_map(|(_, given)| match given {
            Given::Parameter(at) => Some(*at),
            Given::Closed(_) => None,
        })
        .collect();
    let in_order = written.iter().copied().eq(0..arity);
    let keys: Vec<Name> = fields.iter().map(|(key, _)| *key).collect();
    // The parent's methods are this class's too, until it writes one of the
    // same name — and then ITS is the one read, whether or not it could be
    // rewritten, so the parent's is removed before its own is tried.
    let mut methods: BTreeMap<Name, Rc<Method>> = match &parent {
        Some(parent) => parent.methods.clone(),
        None => BTreeMap::new(),
    };
    methods.retain(|name, _| {
        !keys.contains(name) && !written_methods.iter().any(|(held, _)| held == name)
    });
    for (name, function) in &written_methods {
        // Written twice, the second is the one installed; shadowed by a field,
        // neither is what `o.name` reads. Both are left to the call.
        let once = written_methods.iter().filter(|(held, _)| held == name).count() == 1;
        if !once || keys.contains(name) {
            continue;
        }
        if let Some(method) = method_of(function, &keys) {
            methods.insert(*name, Rc::new(method));
        }
    }
    Some(Layout {
        parameters: arity,
        fields,
        methods,
        in_order,
    })
}

/// `m(a, b) { return <expression over this.k, a, b>; }`, and nothing else.
fn method_of(function: &crate::syntax::Function, fields: &[Name]) -> Option<Method> {
    if function.is_async || function.is_generator || function.rest_parameter.is_some() {
        return None;
    }
    let mut parameters = Vec::new();
    for parameter in &function.parameters {
        let Pattern::Name(name) = &parameter.target else {
            return None;
        };
        if parameter.default.is_some() || parameters.contains(name) {
            return None;
        }
        parameters.push(*name);
    }
    let answer = match &function.body {
        FunctionBody::Expression(value) => &**value,
        FunctionBody::Block(body) => match &body[..] {
            [Stmt {
                kind: StmtKind::Return(Some(value)),
                ..
            }] => value,
            _ => return None,
        },
    };
    reads_only(answer, &parameters, fields).then(|| Method {
        parameters,
        answer: answer.clone(),
    })
}

/// Whether `expr` is built from the fields, the parameters and literals by
/// operators alone. An allow-list: a call, a bare `this`, a name from outside,
/// a nested function and a write are all absent from it.
fn reads_only(expr: &Expr, parameters: &[Name], fields: &[Name]) -> bool {
    let again = |inner: &Expr| reads_only(inner, parameters, fields);
    match &expr.kind {
        ExprKind::Literal(Literal::Regex { .. }) => false,
        ExprKind::Literal(_) => true,
        ExprKind::Ident(name) => parameters.contains(name),
        ExprKind::Member {
            object,
            property,
            optional: false,
        } => matches!(object.kind, ExprKind::This) && fields.contains(property),
        ExprKind::Unary { op, operand } => op.reads_a_value() && again(operand),
        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            again(left) && again(right)
        }
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => again(condition) && again(then_branch) && again(else_branch),
        _ => false,
    }
}

/// `answer` with `this.k` spelled `object.k` and each parameter spelled as the
/// argument written for it. Total over what [`reads_only`] admits.
fn spelled(answer: &Expr, method: &Method, written: &[&Expr], object: Name) -> Expr {
    let again = |inner: &Expr| Box::new(spelled(inner, method, written, object));
    let kind = match &answer.kind {
        ExprKind::Ident(name) => {
            let position = method.parameters.iter().position(|held| held == name);
            return match position.and_then(|at| written.get(at)) {
                Some(value) => (*value).clone(),
                None => undefined(answer.at),
            };
        }
        ExprKind::Member { property, .. } => ExprKind::Member {
            object: Box::new(Expr {
                kind: ExprKind::Ident(object),
                at: answer.at,
            }),
            property: *property,
            optional: false,
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
    Expr {
        kind,
        at: answer.at,
    }
}

/// Records one write. Something closed may be written over — it names nothing,
/// so dropping it cannot be seen — and a parameter may not: the argument it
/// stands for has to be evaluated, and a field is the only place this has to
/// evaluate it in.
fn write(fields: &mut Vec<(Name, Given)>, key: Name, given: Given) -> Option<()> {
    match fields.iter_mut().find(|(held, _)| *held == key) {
        None => fields.push((key, given)),
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

impl Layout {
    /// What `object.name(arguments)` answers, as an expression over the fields.
    fn call(&self, object: Name, name: Name, arguments: &[Spreadable]) -> Option<Expr> {
        let method = self.methods.get(&name)?;
        let mut written = Vec::with_capacity(arguments.len());
        for argument in arguments {
            match argument {
                Spreadable::Single(value) if inert(value) => written.push(value),
                _ => return None,
            }
        }
        Some(spelled(&method.answer, method, &written, object))
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
    let mut names = BTreeMap::new();
    rewrite_list(&mut tried, layouts, None, &mut names, prototype);
    if names.is_empty() {
        return None;
    }
    // A name a nested function mentions is one that function can see whole.
    let candidates: Vec<Name> = names.keys().copied().collect();
    let captured = super::capture::captured(&tried, &candidates, &BTreeSet::new());
    let flattened = super::escape::analyse(&tried, parameters, &captured);
    let kept: BTreeSet<Name> = names
        .keys()
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
    rewrite_list(&mut settled, layouts, Some(&kept), &mut BTreeMap::new(), prototype);
    Some(settled)
}

/// The instances rewritten so far, and the layout each was given.
type Instances = BTreeMap<Name, Rc<Layout>>;

/// Rewrites the declarations of one statement list, and of the lists inside it.
/// A nested function or class is not entered: it is rewritten when IT is
/// emitted. `with` is not entered either — a name inside one may be a property.
fn rewrite_list(
    list: &mut Vec<Stmt>,
    layouts: &BTreeMap<Name, Rc<Layout>>,
    only: Option<&BTreeSet<Name>>,
    names: &mut Instances,
    prototype: Name,
) {
    let mut at = 0;
    while at < list.len() {
        let mut checks = Vec::new();
        // The calls first: an instance is declared above what calls it, so
        // every receiver this statement names is already in `names`.
        calls_in_statement(&mut list[at], names);
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
                    let Some(layout) = layouts.get(class) else {
                        continue;
                    };
                    let Some(literal) = layout.literal(arguments, *position) else {
                        continue;
                    };
                    let layout = layout.clone();
                    checks.push(Expr {
                        kind: ExprKind::Member {
                            object: callee.clone(),
                            property: prototype,
                            optional: false,
                        },
                        at: *position,
                    });
                    names.insert(*name, layout);
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
    names: &mut Instances,
    prototype: Name,
) {
    if let StmtKind::Block(inner) = &mut statement.kind {
        rewrite_list(inner, layouts, only, names, prototype);
    }
}

/// Rewrites the method calls in the expressions one statement holds itself.
/// The statements inside it are `rewrite_list`'s, which reaches each in turn.
fn calls_in_statement(statement: &mut Stmt, names: &Instances) {
    if names.is_empty() {
        return;
    }
    match &mut statement.kind {
        StmtKind::Expr(value) | StmtKind::Throw(value) | StmtKind::Return(Some(value)) => {
            calls_in(value, names);
        }
        StmtKind::Declare { bindings, .. } => {
            for binding in bindings {
                if let Some(value) = &mut binding.value {
                    calls_in(value, names);
                }
            }
        }
        StmtKind::If { condition, .. }
        | StmtKind::While { condition, .. }
        | StmtKind::DoWhile { condition, .. } => calls_in(condition, names),
        StmtKind::For {
            init, test, update, ..
        } => {
            match init {
                Some(crate::syntax::ForInit::Expr(value)) => calls_in(value, names),
                Some(crate::syntax::ForInit::Declare { bindings, .. }) => {
                    for binding in bindings {
                        if let Some(value) = &mut binding.value {
                            calls_in(value, names);
                        }
                    }
                }
                None => {}
            }
            for value in [test, update].into_iter().flatten() {
                calls_in(value, names);
            }
        }
        StmtKind::ForEach { subject, .. } => calls_in(subject, names),
        StmtKind::Switch {
            discriminant,
            clauses,
        } => {
            calls_in(discriminant, names);
            for clause in clauses {
                if let Some(test) = &mut clause.test {
                    calls_in(test, names);
                }
            }
        }
        _ => {}
    }
}

/// Rewrites every `o.m(…)` in `expr` whose receiver is an instance with that
/// method. A kind not named here is left as written, calls and all — and a call
/// left as written names its receiver, which is a use `escape` refuses. So what
/// this misses costs a rewrite and never an answer.
fn calls_in(expr: &mut Expr, names: &Instances) {
    if let ExprKind::Call {
        callee,
        arguments,
        optional: false,
    } = &expr.kind
        && let ExprKind::Member {
            object,
            property,
            optional: false,
        } = &callee.kind
        && let ExprKind::Ident(receiver) = &object.kind
        && let Some(answer) = names
            .get(receiver)
            .and_then(|layout| layout.call(*receiver, *property, arguments))
    {
        *expr = answer;
        return;
    }
    match &mut expr.kind {
        ExprKind::Unary { operand, .. } => calls_in(operand, names),
        ExprKind::Binary { left, right, .. } | ExprKind::Logical { left, right, .. } => {
            calls_in(left, names);
            calls_in(right, names);
        }
        ExprKind::Conditional {
            condition,
            then_branch,
            else_branch,
        } => {
            calls_in(condition, names);
            calls_in(then_branch, names);
            calls_in(else_branch, names);
        }
        ExprKind::Member { object, .. } => calls_in(object, names),
        ExprKind::Index { object, index, .. } => {
            calls_in(object, names);
            calls_in(index, names);
        }
        ExprKind::Call {
            callee, arguments, ..
        }
        | ExprKind::New { callee, arguments } => {
            calls_in(callee, names);
            for argument in arguments {
                let (Spreadable::Single(value) | Spreadable::Spread(value)) = argument;
                calls_in(value, names);
            }
        }
        ExprKind::Assign { value, .. } => calls_in(value, names),
        ExprKind::Sequence { operands } => {
            for operand in operands {
                calls_in(operand, names);
            }
        }
        ExprKind::Template { expressions, .. } => {
            for value in expressions {
                calls_in(value, names);
            }
        }
        ExprKind::Array { elements } => {
            for element in elements.iter_mut().flatten() {
                let (Spreadable::Single(value) | Spreadable::Spread(value)) = element;
                calls_in(value, names);
            }
        }
        ExprKind::Object { properties } => {
            for property in properties {
                if let Property::Value { value, .. } = property {
                    calls_in(value, names);
                }
            }
        }
        ExprKind::Await(value) | ExprKind::Chain(value) => calls_in(value, names),
        ExprKind::Asserted { value, .. } => calls_in(value, names),
        _ => {}
    }
}
