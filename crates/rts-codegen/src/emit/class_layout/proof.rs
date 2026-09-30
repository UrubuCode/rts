//! Which class has a layout, and what each of its fields and methods is.

use super::*;

/// Every class DECLARATION in `statement`, in the order written — which is the
/// order a parent and the class that extends it are declared in.
pub(super) fn classes_in<'a>(statement: &'a Stmt, found: &mut Vec<&'a Class>) {
    if let StmtKind::Class(class) = &statement.kind {
        found.push(class);
    }
    walk_stmt(statement, &mut |child| match child {
        StmtChild::Stmt(inner) => classes_in(inner, found),
        StmtChild::Expr(value) => classes_in_expr(value, found),
        StmtChild::Binding(binding) => {
            if let Some(value) = &binding.value {
                classes_in_expr(value, found);
            }
        }
        StmtChild::Catch(clause) => {
            for inner in &clause.body {
                classes_in(inner, found);
            }
        }
        StmtChild::Function(function) => classes_in_function(function, found),
        // A class's own body is not entered: a class declared inside a method
        // is reached through that method's `this`, which this pass does not
        // reason about.
        StmtChild::Class(_) => {}
    });
}

pub(super) fn classes_in_expr<'a>(expr: &'a Expr, found: &mut Vec<&'a Class>) {
    walk_expr(expr, &mut |child| match child {
        Child::Expr(inner) => classes_in_expr(inner, found),
        Child::Function(function) => classes_in_function(function, found),
        Child::Class(_) => {}
    });
}

pub(super) fn classes_in_function<'a>(function: &'a crate::syntax::Function, found: &mut Vec<&'a Class>) {
    if let FunctionBody::Block(body) = &function.body {
        for statement in body {
            classes_in(statement, found);
        }
    }
}

pub(super) fn layout_of(
    class: &Class,
    names: &Names,
    found: &BTreeMap<Name, Rc<Layout>>,
    math: Option<Name>,
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
        if let Some(method) = method_of(function, &keys, math) {
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

/// `m(a, b) { this.k = …; this.j += …; return <expression>; }`: writes to its
/// own fields, then at most one answer, each over `this.k`, `a` and `b`.
pub(super) fn method_of(
    function: &crate::syntax::Function,
    fields: &[Name],
    math: Option<Name>,
) -> Option<Method> {
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
    let reading = Reading {
        parameters: &parameters,
        fields,
        // A parameter spelled `Math` is the parameter.
        math: math.filter(|name| !parameters.contains(name)),
    };
    let (written, answer) = match &function.body {
        FunctionBody::Expression(value) => (&[][..], Some(&**value)),
        FunctionBody::Block(body) => match body.split_last() {
            None => return None,
            Some((
                Stmt {
                    kind: StmtKind::Return(answer),
                    ..
                },
                before,
            )) => (before, answer.as_ref()),
            Some(_) => (&body[..], None),
        },
    };
    let mut writes = Vec::with_capacity(written.len());
    for statement in written {
        let StmtKind::Expr(write) = &statement.kind else {
            return None;
        };
        if !writes_a_field(write, &reading) {
            return None;
        }
        writes.push(write.clone());
    }
    if let Some(answer) = answer
        && !reads_only(answer, &reading)
    {
        return None;
    }
    Some(Method {
        parameters,
        writes,
        answer: answer.cloned(),
    })
}

/// What a method's expressions are read against.
pub(super) struct Reading<'a> {
    parameters: &'a [Name],
    fields: &'a [Name],
    math: Option<Name>,
}

/// `this.k = value`, `this.k += value`, `this.k++`: one of the class's own
/// fields written, from something that only reads.
///
/// Not the logical forms — `this.k ??= v` evaluates its value only sometimes,
/// which is still a read and would be legal; it is left out because nothing
/// measured asks for it.
fn writes_a_field(expr: &Expr, reading: &Reading) -> bool {
    let own = |place: &Expr| {
        matches!(&place.kind, ExprKind::Member { object, property, optional: false }
            if matches!(object.kind, ExprKind::This) && reading.fields.contains(property))
    };
    match &expr.kind {
        ExprKind::Assign {
            target: AssignTarget::Place(place),
            value,
            op: AssignOp::Plain | AssignOp::Compound(_),
        } => own(place) && reads_only(value, reading),
        ExprKind::Update { target, .. } => own(target),
        _ => false,
    }
}

/// Whether `expr` is built from the fields, the parameters and literals by
/// operators — and by `Math`'s functions, where `Math` is the language's.
/// An allow-list: any other call, a bare `this`, a name from outside, a nested
/// function and a write are all absent from it.
pub(super) fn reads_only(expr: &Expr, reading: &Reading) -> bool {
    let again = |inner: &Expr| reads_only(inner, reading);
    match &expr.kind {
        ExprKind::Literal(Literal::Regex { .. }) => false,
        ExprKind::Literal(_) => true,
        ExprKind::Ident(name) => reading.parameters.contains(name),
        ExprKind::Member {
            object,
            property,
            optional: false,
        } => matches!(object.kind, ExprKind::This) && reading.fields.contains(property),
        ExprKind::Call {
            callee,
            arguments,
            optional: false,
        } => {
            matches!(&callee.kind, ExprKind::Member { object, optional: false, .. }
                if matches!(&object.kind, ExprKind::Ident(name) if Some(*name) == reading.math))
                && arguments.iter().all(|argument| match argument {
                    Spreadable::Single(value) => again(value),
                    Spreadable::Spread(_) => false,
                })
        }
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
pub(super) fn spelled(answer: &Expr, method: &Method, written: &[&Expr], object: Name) -> Expr {
    let again = |inner: &Expr| Box::new(spelled(inner, method, written, object));
    let kind = match &answer.kind {
        ExprKind::Ident(name) => {
            let position = method.parameters.iter().position(|held| held == name);
            return match position.and_then(|at| written.get(at)) {
                Some(value) => (*value).clone(),
                None => undefined(answer.at),
            };
        }
        // `this.k`, the only member of `this` admitted; any other member is
        // `Math`'s, inside a call, and is left as written.
        ExprKind::Member {
            object: held,
            property,
            ..
        } if matches!(held.kind, ExprKind::This) => ExprKind::Member {
            object: Box::new(Expr {
                kind: ExprKind::Ident(object),
                at: answer.at,
            }),
            property: *property,
            optional: false,
        },
        ExprKind::Member { .. } => answer.kind.clone(),
        ExprKind::Call {
            callee,
            arguments,
            optional,
        } => ExprKind::Call {
            callee: callee.clone(),
            arguments: arguments
                .iter()
                .map(|argument| match argument {
                    Spreadable::Single(value) => {
                        Spreadable::Single(spelled(value, method, written, object))
                    }
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
        ExprKind::Update {
            op,
            position,
            target,
        } => ExprKind::Update {
            op: *op,
            position: *position,
            target: again(target),
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
pub(super) fn write(fields: &mut Vec<(Name, Given)>, key: Name, given: Given) -> Option<()> {
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
pub(super) fn field_write(statement: &Stmt) -> Option<(Name, &Expr)> {
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
pub(super) fn closed(expr: &Expr) -> bool {
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
pub(super) fn primitive(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Literal(Literal::Regex { .. }) => false,
        ExprKind::Literal(_) => true,
        ExprKind::Unary { op, operand } => op.reads_a_value() && primitive(operand),
        ExprKind::Binary { left, right, .. } => primitive(left) && primitive(right),
        _ => false,
    }
}

pub(super) fn undefined(at: rts_cranelift::fault::Position) -> Expr {
    Expr {
        kind: ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
        at,
    }
}

/// An argument that may be evaluated anywhere, any number of times.
pub(super) fn inert(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Ident(_) | ExprKind::Literal(Literal::Number(_) | Literal::String(_) | Literal::Boolean(_) | Literal::Singleton(_))
    )
}
