//! The rewrite itself: the statement lists walked, and what each site becomes.

use super::*;

/// What the instances that are seen are rewritten with.
pub(super) struct Born<'a> {
    pub(super) layouts: &'a BTreeMap<Name, Rc<Layout>>,
    pub(super) prototype: Name,
}

/// The constructions of one statement list and of the lists inside it, each
/// rewritten to the instance it builds.
pub(super) fn born_in_list(list: &mut [Stmt], names: &Instances, born: &Born, changed: &mut bool) {
    for statement in list {
        calls_in_statement(statement, names, Some(born), changed);
        match &mut statement.kind {
            StmtKind::Block(inner) => born_in_list(inner, names, born, changed),
            StmtKind::If {
                then_branch,
                else_branch,
                ..
            } => {
                born_in_list(std::slice::from_mut(&mut **then_branch), names, born, changed);
                if let Some(other) = else_branch {
                    born_in_list(std::slice::from_mut(&mut **other), names, born, changed);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. }
            | StmtKind::Labelled { body, .. } => {
                born_in_list(std::slice::from_mut(&mut **body), names, born, changed);
            }
            StmtKind::Switch { clauses, .. } => {
                for clause in clauses {
                    born_in_list(&mut clause.body, names, born, changed);
                }
            }
            StmtKind::Try {
                body,
                catch,
                finally,
            } => {
                born_in_list(body, names, born, changed);
                if let Some(catch) = catch {
                    born_in_list(&mut catch.body, names, born, changed);
                }
                if let Some(finally) = finally {
                    born_in_list(finally, names, born, changed);
                }
            }
            _ => {}
        }
    }
}

/// `body` with the instances nothing sees rewritten to their fields, or `None`
/// where there is none.
pub(super) fn unseen(
    layouts: &BTreeMap<Name, Rc<Layout>>,
    body: &[Stmt],
    parameters: &[Name],
    prototype: Name,
) -> Option<Vec<Stmt>> {
    let mut tried = body.to_vec();
    let mut names = BTreeMap::new();
    rewrite_list(&mut tried, layouts, None, &mut names, prototype);
    if names.is_empty() {
        return None;
    }
    // A name a nested function mentions is one that function can see whole.
    let candidates: Vec<Name> = names.keys().copied().collect();
    let captured = crate::emit::capture::captured(&tried, &candidates, &BTreeSet::new());
    let flattened = crate::emit::escape::analyse(&tried, parameters, &captured);
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
pub(super) type Instances = BTreeMap<Name, Rc<Layout>>;

/// Rewrites the declarations of one statement list, and of the lists inside it.
/// A nested function or class is not entered: it is rewritten when IT is
/// emitted. `with` is not entered either — a name inside one may be a property.
pub(super) fn rewrite_list(
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
        calls_in_statement(&mut list[at], names, None, &mut false);
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
pub(super) fn rewrite_boxed(
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
pub(super) fn calls_in_statement(
    statement: &mut Stmt,
    names: &Instances,
    born: Option<&Born>,
    changed: &mut bool,
) {
    if names.is_empty() && born.is_none() {
        return;
    }
    let mut calls_in = |value: &mut Expr, names: &Instances| rewrite_in(value, names, born, changed);
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
pub(super) fn rewrite_in(expr: &mut Expr, names: &Instances, born: Option<&Born>, changed: &mut bool) {
    let mut calls_in = |value: &mut Expr, names: &Instances| rewrite_in(value, names, born, changed);
    // A construction of a class with a layout, where instances are being born:
    // its arguments first, which are rewritten as written, and then itself.
    if let Some(born) = born
        && let ExprKind::New { callee, arguments } = &mut expr.kind
        && let ExprKind::Ident(class) = &callee.kind
        && let Some(layout) = born.layouts.get(class)
    {
        for argument in arguments.iter_mut() {
            let (Spreadable::Single(value) | Spreadable::Spread(value)) = argument;
            calls_in(value, names);
        }
        if let Some(instance) = layout.born(callee, born.prototype, arguments, expr.at) {
            *expr = instance;
            *changed = true;
        }
        return;
    }
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
