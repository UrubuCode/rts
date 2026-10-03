//! `@d class C { @d m() {} }` — decorators, desugared into ordinary syntax.
//!
//! # Which of the two designs, and why
//!
//! The **legacy** design, the one `tsc --experimentalDecorators` implements and
//! the one every decorator in the wild today is written against: a class
//! decorator is called with the constructor and may return a replacement; a
//! method or accessor decorator is called with `(target, key, descriptor)` and
//! may return a descriptor; a field decorator with `(target, key, undefined)`;
//! a parameter decorator with `(target, key, index)`.
//!
//! The ES2022 standard design is a different, incompatible contract — a class
//! decorator there receives `(value, context)`, where `context` is an object
//! describing the element, and a field decorator receives an initialiser
//! transformer rather than a key. The two are **distinguishable from the
//! outside**, which is what settles the choice for a program that wants the
//! other one: measured against bun 1.4.0 on one class with a field, a method,
//! a getter and a static method decorated,
//!
//! ```text
//! legacy (experimentalDecorators: true)  field, method, accessor, static, class
//! ES2022 (the default)                   static, method, accessor, field, class
//! ```
//!
//! so a program written for ES2022 does not merely see different arguments, it
//! sees a different order. There is no `tsconfig` read here, and the engine
//! therefore does not switch: legacy is what is implemented, and a program
//! written against the standard design will see arguments it does not expect
//! rather than a refusal.
//!
//! **And the standard design is what a real library in front of us wants.**
//! `kire`'s `@prop` is
//!
//! ```text
//! function prop(_value: undefined, context: ClassFieldDecoratorContext) {
//!   context.addInitializer(function () { … });
//! }
//! ```
//!
//! — the second parameter is a context object, not a property key, so under the
//! legacy contract `context` is the string `"count"` and `addInitializer` is
//! not a function. Measured 2026-10-03 against bun 1.4.0 on that exact shape:
//! it answers `built 0 1 seen=count,step` with the standard design and throws
//! `context.addInitializer is not a function` with `experimentalDecorators`.
//! So the two designs are not "one modern, one legacy, pick either" — a
//! program is written for exactly one, and this engine implements the one
//! `kire` is not written for.
//!
//! What saves that from being a silent wrong answer is accidental rather than
//! designed: the mismatch throws, loudly, at the decoration. What would make it
//! a decision is reading `experimentalDecorators` out of a `tsconfig.json` and
//! refusing the other design by name, and that is the next lot together with
//! the standard contract itself — a field initialiser transformer, an
//! `addInitializer` queue that runs per instance, and `context.access`. None of
//! those is expressible as this file's shape of desugaring, which is why it is
//! a lot and not an afternoon.
//!
//! # Why a desugaring in `parse/` rather than a stage in `emit/`
//!
//! Because a decorator adds no operation. Everything it needs — a call, an
//! assignment, `Object.getOwnPropertyDescriptor`, `Object.defineProperty`, a
//! `var` that escapes a block — is already a node this tree has and both
//! pipelines already compile. Lowering it to those nodes means the MIR path and
//! the `emit/` path cannot disagree about it, which is the failure RULE 0a
//! exists to prevent; writing it as a stage would have been a seventh language
//! feature implemented twice.
//!
//! # What the previous version did, and the two defects it had
//!
//! It lowered a class decorator and dropped every member decorator silently.
//! Of the two things it did do, both were wrong, and the module's own
//! documentation stated the first as a decision:
//!
//! - A decorator written as a **factory call** (`@Entity("user")`) had the call
//!   evaluated and the function it returned thrown away — "the call form is
//!   treated as the whole decoration". So no decorator in the ordinary spelling
//!   ever ran. The reason recorded for it was that `decorator_factory`'s own
//!   fixture returns `0`, which is not callable — true, and a statement about
//!   that fixture rather than about the language. The fixture was wrong; bun
//!   throws on it.
//! - A decorator returning `undefined` **replaced the class with `undefined`**,
//!   because the assignment was unconditional. Legacy semantics keep the class
//!   unless a value came back, which is why `__decorate` is written `r = d(r)
//!   || r`.
use swc_ecma_ast as swc;

use super::expr::expr;
use super::item::class_parts;
use super::{Cx, Result, position, unsupported};
use crate::names::Name;
use crate::syntax::{
    AssignOp, AssignTarget, Binding, BindingKind, Class, Expr, ExprKind, Literal, LogicalOp,
    Pattern, Spreadable, Stmt, StmtKind, Text,
};
use crate::values::Singleton;

type At = rts_cranelift::fault::Position;

/// The scratch binding a member's decoration writes its descriptor into.
///
/// A `let` inside a block of its own, one per decorated member. Deliberately
/// not a `var` beside the class binding: `var` would escape into the module
/// scope, and the module scope is where `emit::module::declared_names` reads an
/// exported declaration's names from — so `export @D class C {}` would have
/// published this scratch name as an export of the module. A per-member `let`
/// cannot be seen from outside the member's own block, so nothing has to agree
/// about filtering it out.
const DESCRIPTOR: &str = "__rts_decorator_descriptor";

/// `@d1 @d2 class C { … }`.
///
/// Lowered to, for `@C1 @C2 class K { @F f = 1; @M m(@P x) {} @S static s() {} }`:
///
/// `_d` below stands for [`DESCRIPTOR`], and each member's group is a block of
/// its own so that `_d` is a `let` nobody outside it can see:
///
/// ```text
/// var K;
/// K = class K { … };
///
/// { let _d;                                        // the field: no descriptor
///   _d = undefined;
///   _d = F(K.prototype, "f", _d) || _d;
///   if (_d) Object.defineProperty(K.prototype, "f", _d); }
///
/// P(K.prototype, "m", 0);                          // parameters run first
/// { let _d;
///   _d = Object.getOwnPropertyDescriptor(K.prototype, "m");
///   _d = M(K.prototype, "m", _d) || _d;
///   if (_d) Object.defineProperty(K.prototype, "m", _d); }
///
/// …the same for `s`, with `K` in place of `K.prototype`…
///
/// K = C2(K) || K;                                  // the class last
/// K = C1(K) || K;                                  // nearest-declaration first
/// ```
///
/// Three orders are observable and all three are pinned by
/// `claude-decorator-order.test.ts`, measured against bun 1.4.0 with
/// `experimentalDecorators`:
///
/// - **Members in textual order, then the class.** The constructor is not a
///   member here: its parameter decorators belong to the class's own group and
///   run last, after every other member and before the class decorators.
/// - **Several decorators on one target run bottom-up**, the one nearest the
///   declaration first. That is `__decorate` iterating its array backwards.
/// - **A member's parameter decorators run before the member's own**, in
///   descending index order, because `__decorate` receives them appended after
///   the member's decorators and walks the array backwards.
pub(super) fn decorated_class_declaration(
    cx: &mut Cx,
    class: &swc::ClassDecl,
    at: At,
) -> Result<Stmt> {
    let name = cx.name(&class.ident.sym);
    let class_value = Class {
        name: Some(name),
        ..class_parts(cx, &class.class)?
    };

    let mut stmts = vec![
        Stmt::new(
            StmtKind::Declare {
                kind: BindingKind::Var,
                bindings: vec![declare(name)],
            },
            at,
        ),
        Stmt::new(
            StmtKind::Expr(assign(
                ident(name, at),
                Expr::new(ExprKind::Class(Box::new(class_value)), at),
                at,
            )),
            at,
        ),
    ];

    // Every member but the constructor, in the order they are written.
    for member in &class.class.body {
        member_decorations(cx, member, name, &mut stmts)?;
    }

    // The class group: the constructor's parameter decorators, highest index
    // first, and then the class decorators nearest-declaration first.
    let constructor = class.class.body.iter().find_map(|member| match member {
        swc::ClassMember::Constructor(constructor) => Some(constructor),
        _ => None,
    });
    if let Some(constructor) = constructor {
        for (index, parameter) in constructor.params.iter().enumerate().rev() {
            let decorators = match parameter {
                swc::ParamOrTsParamProp::Param(param) => &param.decorators[..],
                swc::ParamOrTsParamProp::TsParamProp(property) => &property.decorators[..],
            };
            for decorator in decorators.iter().rev() {
                let at = position(decorator.span);
                let callee = expr(cx, &decorator.expr)?;
                // A constructor parameter's key is `undefined`: the parameter
                // belongs to the class, which has no key of its own. Measured
                // against bun 1.4.0, where a method parameter sees the method's
                // name and a constructor parameter sees `undefined`.
                stmts.push(Stmt::new(
                    StmtKind::Expr(call(
                        callee,
                        vec![
                            ident(name, at),
                            Expr::new(
                                ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
                                at,
                            ),
                            Expr::new(ExprKind::Literal(Literal::Number(index as f64)), at),
                        ],
                        at,
                    )),
                    at,
                ));
            }
        }
    }

    for decorator in class.class.decorators.iter().rev() {
        let at = position(decorator.span);
        let callee = expr(cx, &decorator.expr)?;
        // `|| K` and not a bare assignment: a decorator that returns nothing is
        // an observer, and the class it observed survives it.
        let applied = or_else(call(callee, vec![ident(name, at)], at), ident(name, at), at);
        stmts.push(Stmt::new(
            StmtKind::Expr(assign(ident(name, at), applied, at)),
            at,
        ));
    }

    Ok(Stmt::new(StmtKind::Block(stmts), at))
}

/// Whether anything in this class is decorated — the class, a member, or a
/// parameter of a member or of the constructor.
///
/// The routing question, and getting it wrong is how a member decorator can
/// still be dropped after being implemented: `parse::stmt` used to send a class
/// down the decorated path only when the CLASS itself carried one, so
/// `class Svc { @Wrap go() {} }` took the ordinary path and every member
/// decorator in it vanished. Measured 2026-10-03 — `@Wrap` never ran, where bun
/// wrapped the method.
pub(super) fn is_decorated_class(class: &swc::Class) -> bool {
    if !class.decorators.is_empty() {
        return true;
    }
    class.body.iter().any(|member| match member {
        swc::ClassMember::Constructor(constructor) => {
            constructor.params.iter().any(|parameter| match parameter {
                swc::ParamOrTsParamProp::Param(param) => !param.decorators.is_empty(),
                swc::ParamOrTsParamProp::TsParamProp(property) => {
                    !property.decorators.is_empty()
                }
            })
        }
        other => is_decorated(other),
    })
}

/// Whether a class member carries any decorator at all, on itself or on one of
/// its parameters.
///
/// Asked before anything is built so that an undecorated body costs nothing: a
/// class with a decorator on itself and none on its members emits no member
/// group, which is also what keeps `decorator_class.test.ts`'s emitted shape
/// what it was.
fn is_decorated(member: &swc::ClassMember) -> bool {
    let parameters_decorated = |function: &swc::Function| {
        function
            .params
            .iter()
            .any(|parameter| !parameter.decorators.is_empty())
    };
    match member {
        swc::ClassMember::Method(method) => {
            !method.function.decorators.is_empty() || parameters_decorated(&method.function)
        }
        swc::ClassMember::PrivateMethod(method) => {
            !method.function.decorators.is_empty() || parameters_decorated(&method.function)
        }
        swc::ClassMember::ClassProp(property) => !property.decorators.is_empty(),
        swc::ClassMember::PrivateProp(property) => !property.decorators.is_empty(),
        // A constructor's own decorators do not exist in the grammar, and its
        // parameters' belong to the class group rather than to a member group.
        _ => false,
    }
}

/// The statements that decorate one member, appended in the order they run.
fn member_decorations(
    cx: &mut Cx,
    member: &swc::ClassMember,
    class: Name,
    outer: &mut Vec<Stmt>,
) -> Result<()> {
    if !is_decorated(member) {
        return Ok(());
    }

    let (key, is_static, own_decorators, parameters, at, has_descriptor) = match member {
        swc::ClassMember::Method(method) => (
            key_text(&method.key)?,
            method.is_static,
            &method.function.decorators[..],
            &method.function.params[..],
            position(method.span),
            true,
        ),
        swc::ClassMember::ClassProp(property) => (
            key_text(&property.key)?,
            property.is_static,
            &property.decorators[..],
            &[][..],
            position(property.span),
            false,
        ),
        // A `#private` member has no property key, so there is nothing a
        // decorator could be handed as its second argument and no descriptor to
        // read. TypeScript refuses the same program, and refusing is the only
        // honest answer: running the decorator with an invented key would make
        // a registry record a member nothing can reach.
        swc::ClassMember::PrivateMethod(method) => {
            return unsupported("a decorator on a private method", position(method.span));
        }
        swc::ClassMember::PrivateProp(property) => {
            return unsupported("a decorator on a private field", position(property.span));
        }
        // Unreachable, and held so by `is_decorated` above: it answers true
        // only for these four variants, and nothing else in a class body can
        // carry a decorator SWC will hand over. Written as a return rather than
        // an `unreachable!` because the cost of being wrong here is a dropped
        // decorator in a release build, not a panic in a test.
        _ => return Ok(()),
    };

    // The target: the prototype for an instance member, the constructor for a
    // static one. Rebuilt per use rather than held in a second scratch binding
    // — `K.prototype` is a property read of a local, which is cheaper than the
    // binding would be and cannot be observed differently.
    let prototype = cx.name("prototype");
    let target = |at: At| {
        if is_static {
            ident(class, at)
        } else {
            Expr::new(
                ExprKind::Member {
                    object: Box::new(ident(class, at)),
                    property: prototype,
                    optional: false,
                },
                at,
            )
        }
    };
    let key_expr = |at: At| Expr::new(ExprKind::Literal(Literal::String(key.clone())), at);

    // Parameters first, highest index first.
    for (index, parameter) in parameters.iter().enumerate().rev() {
        for decorator in parameter.decorators.iter().rev() {
            let at = position(decorator.span);
            let callee = expr(cx, &decorator.expr)?;
            outer.push(Stmt::new(
                StmtKind::Expr(call(
                    callee,
                    vec![
                        target(at),
                        key_expr(at),
                        Expr::new(ExprKind::Literal(Literal::Number(index as f64)), at),
                    ],
                    at,
                )),
                at,
            ));
        }
    }

    if own_decorators.is_empty() {
        return Ok(());
    }

    let descriptor = cx.name(DESCRIPTOR);
    let mut group = vec![Stmt::new(
        StmtKind::Declare {
            kind: BindingKind::Let,
            bindings: vec![declare(descriptor)],
        },
        at,
    )];
    let stmts = &mut group;

    // The descriptor a method or accessor decorator receives is the one the
    // class already installed; a field has none, and its decorator sees
    // `undefined` there. Measured against bun 1.4.0: a field decorator is
    // called with three arguments whose third is `undefined`.
    let initial = if has_descriptor {
        object_call(
            cx,
            "getOwnPropertyDescriptor",
            vec![target(at), key_expr(at)],
            at,
        )?
    } else {
        Expr::new(
            ExprKind::Literal(Literal::Singleton(Singleton::Undefined)),
            at,
        )
    };
    stmts.push(Stmt::new(
        StmtKind::Expr(assign(ident(descriptor, at), initial, at)),
        at,
    ));

    for decorator in own_decorators.iter().rev() {
        let at = position(decorator.span);
        let callee = expr(cx, &decorator.expr)?;
        let applied = or_else(
            call(
                callee,
                vec![target(at), key_expr(at), ident(descriptor, at)],
                at,
            ),
            ident(descriptor, at),
            at,
        );
        stmts.push(Stmt::new(
            StmtKind::Expr(assign(ident(descriptor, at), applied, at)),
            at,
        ));
    }

    // Guarded rather than unconditional, and the guard is what `__decorate`
    // writes as `c > 3 && r &&`: a field whose decorators all returned nothing
    // has no descriptor to install, and `defineProperty(…, undefined)` throws.
    let install = Stmt::new(
        StmtKind::Expr(object_call(
            cx,
            "defineProperty",
            vec![target(at), key_expr(at), ident(descriptor, at)],
            at,
        )?),
        at,
    );
    stmts.push(Stmt::new(
        StmtKind::If {
            condition: ident(descriptor, at),
            then_branch: Box::new(install),
            else_branch: None,
        },
        at,
    ));

    outer.push(Stmt::new(StmtKind::Block(group), at));
    Ok(())
}

/// The member's key as the string a decorator is handed.
///
/// A computed key is refused rather than rebuilt: the expression was already
/// evaluated once when the class was defined, and evaluating it a second time
/// here would run its side effects twice and could answer a different key.
/// Holding the first answer needs one scratch binding per computed member,
/// which is the next lot rather than this one.
fn key_text(key: &swc::PropName) -> Result<Text> {
    Ok(match key {
        swc::PropName::Ident(ident) => units(&ident.sym.to_string()),
        swc::PropName::Str(string) => {
            Text::from_units(string.value.as_wtf8().to_ill_formed_utf16().collect())
        }
        swc::PropName::Num(number) => units(&crate::parse::expr::number_key(number.value)),
        swc::PropName::BigInt(big) => units(&big.value.to_string()),
        swc::PropName::Computed(computed) => {
            return unsupported(
                "a decorator on a member with a computed key",
                position(computed.span),
            );
        }
    })
}

fn units(text: &str) -> Text {
    Text::from_units(text.encode_utf16().collect())
}

fn declare(name: Name) -> Binding {
    Binding {
        target: Pattern::Name(name),
        value: None,
        claim: None,
    }
}

fn ident(name: Name, at: At) -> Expr {
    Expr::new(ExprKind::Ident(name), at)
}

fn assign(target: Expr, value: Expr, at: At) -> Expr {
    Expr::new(
        ExprKind::Assign {
            target: AssignTarget::Place(Box::new(target)),
            value: Box::new(value),
            op: AssignOp::Plain,
        },
        at,
    )
}

fn or_else(left: Expr, right: Expr, at: At) -> Expr {
    Expr::new(
        ExprKind::Logical {
            op: LogicalOp::Or,
            left: Box::new(left),
            right: Box::new(right),
        },
        at,
    )
}

fn call(callee: Expr, arguments: Vec<Expr>, at: At) -> Expr {
    Expr::new(
        ExprKind::Call {
            callee: Box::new(callee),
            arguments: arguments.into_iter().map(Spreadable::Single).collect(),
            optional: false,
        },
        at,
    )
}

/// `Object.<member>(…)`.
///
/// Named through the global rather than through an entry point because the
/// desugaring must mean exactly what the program would mean if it had written
/// the call itself — a program that replaced `Object.defineProperty` sees its
/// own function here, as it does in every other engine.
fn object_call(cx: &mut Cx, member: &str, arguments: Vec<Expr>, at: At) -> Result<Expr> {
    let object = cx.name("Object");
    let property = cx.name(member);
    Ok(call(
        Expr::new(
            ExprKind::Member {
                object: Box::new(ident(object, at)),
                property,
                optional: false,
            },
            at,
        ),
        arguments,
        at,
    ))
}
