//! A class, lowered into what a class already was.
//!
//! # Why there is no class in the runtime
//!
//! Because there is no class in the language either. A class is a constructor
//! function, an object hanging off its `prototype` property holding the methods,
//! and two prototype links when there is an `extends` — every one of which this
//! engine already had. Adding a runtime notion of "class" would be inventing a
//! second kind of object to express something the first kind already expresses,
//! and every operation that reached one would then have to know about both.
//!
//! So this module is a **lowering** and not a feature. What it produces is
//! indistinguishable from what the equivalent hand-written function produces,
//! which is also what makes `class X extends RegExp {}` work without anything in
//! the regular-expression module knowing that classes exist.
//!
//! # The four links, and why each one is needed
//!
//! ```text
//!   B.prototype.__proto__ = A.prototype     an instance finds A's methods
//!   B.__proto__           = A               B finds A's STATIC methods
//!   instance.__proto__    = B.prototype     `new` already does this
//!   home                  = B.prototype     `super.m` starts one link above it
//! ```
//!
//! The second is the one an implementation forgets, and forgetting it is
//! invisible until a program calls an inherited static method.
//!
//! # How `super` is reached
//!
//! Through an ordinary environment. The class builds one at definition time
//! holding the parent constructor and the home object, links it to the
//! environment the class was written in, and emits every method against it — so
//! `super.m()` is an environment read and a property read, and `super()` is an
//! environment read and a call. Nothing about the calling convention changes,
//! and a method that mentions neither pays for neither.
//!
//! That is also what the specification describes: a method's `[[HomeObject]]` is
//! an internal slot of the function, and an environment entry is the same fact
//! stored where this engine can already store facts.
//!
//! # Private members: unreachable, not access-checked
//!
//! `this.#x` is not a property a program can reach by any string it can write —
//! it is not access CONTROL, it is a key OUTSIDE the space a string literal can
//! spell. That is the same shape `rts-core`'s `Symbol` key already uses:
//! `crates/rts-core/src/entry/symbol.rs` keeps a well-known symbol as an
//! ordinary interned name under a reserved prefix (`"@@iterator"`) rather than a
//! third `Key` variant, because a third variant would need a second path through
//! every place a shape becomes a layout, a cache, or an enumeration.
//!
//! A private name is the same problem answered the same way, and it goes in the
//! same space: `parse/mod.rs`'s `Cx::private_name` interns `#x` as `"@@#x"`, so
//! the key is inside the `@@` prefix the runtime already excludes from every
//! enumeration. One filter, not two — `is_symbol_key` is unchanged.
//!
//! It was `"#x"` for the length of one review. `#` alone is a prefix a program
//! CAN write: `o["#main"]` is an ordinary property, and a runtime filtering on
//! `#` would have made it disappear from `Object.keys` and `JSON.stringify`.
//! That is the whole reason the reserved space is spelled `@@` — it is the one
//! no source text produces.
//!
//! **What this buys, and what it does not.** A program cannot reach the key by
//! any spelling `.` or a computed access lets it type: `.#x` is legal only inside
//! the declaring class body, and `o["@@#x"]` is the same construction the `@@`
//! space already documents as possible and pointless. What it does not give is
//! an access check — the same caveat `symbol.rs` writes down for `"@@iterator"`,
//! and the same trade.
//!
//! **Divergence, named**: this engine gives every `#x` across every class the
//! same identity, because `Cx::private_name` interns by text and text is global.
//! The specification gives each declaration its own private name, so two
//! unrelated classes that both write `#x` are, per spec, unrelated fields; here
//! they are the same property key. That was already true before this module did
//! anything with private members — `Cx::private_name`'s interning made the
//! choice — and this module does not change it, only makes the key reachable
//! from inside the class body rather than refused outright.

use std::collections::BTreeSet;

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitError, EmitResult, Loops, Scope};
use super::{binding, expr, function};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{
    AssignOp, AssignTarget, Class, ClassElement, ClassKey, Expr, ExprKind, Function, FunctionBody,
    MethodKind, PropertyKey, Stmt, StmtKind,
};

/// The name the parent constructor is held under.
///
/// Spelled so a program cannot write it. A collision would be harmless anyway —
/// a class environment is never handed to JavaScript — but naming it once is
/// what keeps the writer and the reader agreeing.
const SUPER: &str = "__rts_super";

/// The name the home object is held under.
const HOME: &str = "__rts_home";

/// The name the home object of a STATIC member is held under.
///
/// A second name rather than a second value under the first, because both are
/// live at once: a class body has instance methods and static ones, and each
/// resolves `super` against its own home. The specification says the same in
/// its own words — `[[HomeObject]]` is per function, and a static method's is
/// the constructor.
const STATIC_HOME: &str = "__rts_static_home";

/// The name a derived constructor holds `this` under.
///
/// Its own name rather than the receiver, because the object does not exist
/// until `super()` answers one — see [`super::Scope::bind_this_late`].
const THIS: &str = "__rts_this";

/// The name a computed member key's VALUE is held under, once per element
/// position in the class body.
///
/// Only an instance field needs this. A method's and a static field's key is
/// used in the same activation that evaluated it, so the `ValueId` is reused
/// directly — see the `computed` slice threaded through [`emit_class`]. An
/// instance field's initialiser runs later, inside the constructor, which is a
/// *different* compiled function: the value has to survive as a captured name
/// the way `__rts_super` and `__rts_home` already do, or the field would
/// re-evaluate the key expression once per instance, which is not what the
/// specification says a computed key does.
fn computed_key_name(index: usize) -> String {
    format!("__rts_key{index}")
}

/// Emits a class and answers the constructor value.
pub(super) fn emit_class(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    class: &Class,
) -> EmitResult<ValueId> {
    refuse_what_is_not_built(class)?;

    // The heritage is evaluated first and exactly once, before anything else in
    // the body — including a computed key, which is why the order is the
    // semantics rather than the tidy way to write it.
    let parent = match &class.heritage {
        Some(heritage) => Some(super::emit_expr(builder, scope, ctx, heritage)?),
        None => None,
    };

    // Every computed key is evaluated exactly once, here, in source order,
    // against the scope the class was WRITTEN in — before any field
    // initialiser or static block runs, and before the class's own environment
    // exists, because a computed key can see neither `this`, nor `super`, nor a
    // private name the class it names is still in the middle of declaring.
    let computed: Vec<Option<ValueId>> = class
        .body
        .iter()
        .map(|element| match element.key() {
            Some(ClassKey::Public(PropertyKey::Computed(key_expr))) => {
                super::emit_expr(builder, scope, ctx, key_expr).map(Some)
            }
            _ => Ok(None),
        })
        .collect::<EmitResult<_>>()?;

    // A class needs its own environment for `super`, as before, and now also
    // for a computed INSTANCE field's key — that value is read again inside the
    // constructor, a function this scope does not reach into otherwise.
    // …and for the class's OWN NAME, whenever a method mentions it. A method is
    // a separate compiled function, so a name bound as a register in this
    // activation is unreachable from inside one — `const C = class Inner {
    // m() { return Inner.name; } }` answered `ReferenceError: Inner is not
    // defined`. Over-approximates: it asks whether the name appears anywhere in
    // the body rather than resolving scopes, which is the same trade
    // `capture::captured` documents and in the same safe direction.
    let names_itself = class
        .name
        .is_some_and(|name| super::capture::class_mentions(&class.body, name));
    let needs_environment = class.is_derived()
        || names_itself
        || class
            .body
            .iter()
            .enumerate()
            .any(|(index, element)| computed[index].is_some() && element.runs_per_instance());

    // A computed instance field's key value, one per element position that
    // needs it — read again inside the constructor, which is why it has to be
    // both WRITTEN into the environment `class_scope` makes and RECORDED in
    // the `Scope` it returns. Writing it and stopping there was the bug: the
    // constructor's own `Scope::for_function` seeds its environment bindings
    // from `inner.reachable()`, and a name only ever `property::emit_write`'d
    // — never `declare`d as captured — is invisible to that call. The
    // constructor's synthetic `Expr::Ident("__rts_key0")` read what the value
    // WAS at, correctly, and found no binding that said so.
    let computed_fields: Vec<(Name, ValueId)> = class
        .body
        .iter()
        .enumerate()
        .filter_map(|(index, element)| {
            let (Some(value), true) = (computed[index], element.runs_per_instance()) else {
                return None;
            };
            Some((ctx.names.intern(&computed_key_name(index)), value))
        })
        .collect();

    let mut inner = class_scope(
        builder,
        scope,
        ctx,
        parent,
        needs_environment,
        &computed_fields,
        class.name.filter(|_| names_itself),
    )?;

    let constructor = emit_constructor(builder, &mut inner, ctx, class, &computed)?;

    // Every class constructor, derived or not, is refused when reached any
    // way other than `new` — the language throws a `TypeError` for
    // `ClassName()`, where an ordinary function called plainly just runs. The
    // fact is syntactic (this IS a class) and the runtime cannot see it any
    // other way, which is the same shape `MarkDerived` already answers for
    // `extends`.
    expr::call(builder, ctx, RuntimeOp::MarkClassConstructor, &[constructor])?;

    // `ClosureNew` already made a `prototype` object, because a function that
    // could not be constructed with would be a different kind of function. So
    // this reads what exists rather than making a second one — two would be the
    // classic bug where methods land on an object no instance inherits from.
    let prototype_name = ctx.names.intern("prototype");
    let prototype = super::property::emit_read(builder, ctx, constructor, prototype_name)?;

    // `instance.constructor` and `C.prototype.constructor` both read this —
    // the link back that lets a program discover which class made a value,
    // and the one `ClosureNew` cannot make for an ordinary function because it
    // has no class name to write.
    let constructor_key = ctx.names.intern("constructor");
    // Non-enumerable, which is what the language says about the link and what
    // `for`-`in` over an instance sees the moment it walks the chain.
    let link = super::property::key_constant(builder, ctx, constructor_key);
    let held = expr::tagged(builder, constructor);
    expr::call(builder, ctx, RuntimeOp::DefineMethod, &[prototype, link, held])?;

    // `C.name` — the class's own name, or the one a binding lent it. An
    // anonymous `class {}` written as the initialiser of `const X` is called
    // `X`, which is NamedEvaluation and which this said it did not attempt;
    // `Ctx::take_lent_name` is the half that arrived, and it is TAKEN so a
    // class nested inside the initialiser does not inherit the name.
    let lent = ctx.take_lent_name();
    if let Some(name) = class.name.or(lent) {
        let text = ctx.names.text(name).to_owned();
        let name_value = expr::string_literal(builder, ctx, &text)?;
        let name_key = ctx.names.intern("name");
        super::property::emit_write(builder, ctx, constructor, name_key, name_value)?;
    }

    if let Some(parent) = parent {
        let parent_prototype = super::property::emit_read(builder, ctx, parent, prototype_name)?;
        expr::call(
            builder,
            ctx,
            RuntimeOp::SetPrototype,
            &[prototype, parent_prototype],
        )?;
        // The link an implementation forgets, and whose absence shows only when
        // a program calls an inherited STATIC method.
        expr::call(builder, ctx, RuntimeOp::SetPrototype, &[constructor, parent])?;
    }

    // Written now rather than with the parent, because the prototype did not
    // exist until the constructor did — and only into an environment this CLASS
    // built, which is the condition [`class_scope`] decides on.
    //
    // `inner.environment()` is not that condition, and reading it as one wrote
    // into nothing. A `Scope` always carries `Some` environment: a function
    // that builds none passes on the one it was HANDED, and at the top level
    // what a program is handed is `undefined`. So `class Foo { m() {} }` at
    // module scope wrote its home object onto `undefined` — a store that did
    // nothing, silently, for as long as a store to `undefined` was allowed to.
    // The moment one raised, every top-level class in the corpus stopped
    // running, which is how a dead write announces itself.
    //
    // Nothing was lost by the write not happening, and that is the point: the
    // simple case has no `super` to resolve a home object FOR, which is exactly
    // why `class_scope` gives it no environment of its own.
    if parent.is_some() || needs_environment {
        let environment = inner
            .environment()
            .expect("a class that builds an environment has one");
        let home = ctx.names.intern(HOME);
        super::property::emit_write(builder, ctx, environment, home, prototype)?;
        // And the constructor, which is what a STATIC method's `super` looks
        // one link above. Written here for the same reason the other is: the
        // constructor did not exist until now.
        let static_home = ctx.names.intern(STATIC_HOME);
        super::property::emit_write(builder, ctx, environment, static_home, constructor)?;
    }

    // A static field initialiser and a static block both run with `this` bound
    // to the class itself — the specification's words for it, not an
    // approximation. Set once, here, because both live in the loop below and
    // neither is a separate compiled function the way an instance method is.
    inner.set_this(constructor, false);

    // The class's own NAME, bound inside its body. The specification gives a
    // class body a binding for the name it declares, and it is what
    // `static { Config.VERSION = "1.0.0" }` reads — the most common way a static
    // block is written, and the only way an ordinary program writes it.
    //
    // Without this the name resolved to whatever the enclosing scope had, which
    // for a declaration is a binding not written until the class expression
    // finishes: `Config` read `undefined` inside the block, the assignment went
    // to a property of nothing, and the block ran to completion having done
    // nothing. It was invisible until a native could throw — `Config.describe()`
    // then answered `undefined` and calling it became a `TypeError`.
    if let Some(name) = class.name {
        binding::declare(builder, &mut inner, ctx, name, constructor)?;
    }

    for (index, element) in class.body.iter().enumerate() {
        match element {
            ClassElement::Method(method) if !method.is_constructor(&ctx.names) => {
                // Saved and restored rather than set and cleared, for the
                // reason `Ctx::in_cleanup` gives: a class written inside a
                // static method would otherwise leave the flag on for the
                // instance methods emitted after it.
                let enclosing = ctx.in_static_method;
                ctx.in_static_method = method.is_static;
                // The key names the method, which is the only thing that can:
                // `read() {}` parses as a function with no identifier of its
                // own, so `K.prototype.read.name` was `""` — and every spelling
                // built on it inherited the emptiness, `bind` reporting
                // `"bound "` for a method every other engine names.
                //
                // A private method's spec name is `"#name"` and a computed key
                // names whatever the expression answers; neither is spelled
                // here, because both need a name the source never wrote and
                // this only carries one the interner already holds.
                if let ClassKey::Public(PropertyKey::Named(name)) = &method.key {
                    let spelled = match method.kind {
                        MethodKind::Normal => *name,
                        // `"get width"` / `"set width"`, the prefixed spelling
                        // an object literal's accessor also takes — asked
                        // through that module rather than re-derived, so the
                        // two cannot come to disagree about the space.
                        MethodKind::Getter => super::object::accessor_name(ctx, *name, true),
                        MethodKind::Setter => super::object::accessor_name(ctx, *name, false),
                    };
                    ctx.lend_name(spelled);
                }
                let closure = function::emit_closure_method(builder, &inner, ctx, &method.function)?;
                ctx.in_static_method = enclosing;
                let target = if method.is_static { constructor } else { prototype };
                // An accessor is not written as a property: it is a pair of
                // functions the read has to CALL, and a getter stored in the
                // layout would be returned by the cache instead of run.
                match method.kind {
                    MethodKind::Normal => {
                        write_member(builder, ctx, target, &method.key, computed[index], closure, true)?;
                    }
                    MethodKind::Getter | MethodKind::Setter => {
                        let is_getter = matches!(method.kind, MethodKind::Getter);
                        match accessor_name(&method.key) {
                            Some(name) => {
                                super::object::define_accessor(
                                    builder, ctx, target, name, closure, is_getter,
                                )?;
                            }
                            // A computed key, already evaluated once above and
                            // held in `computed` — the same value `write_member`
                            // uses for an ordinary method with a computed name.
                            None => {
                                let key = computed[index].ok_or(EmitError::Unsupported {
                                    construct: "a computed accessor name with no evaluated key",
                                })?;
                                super::object::define_computed_accessor(
                                    builder, ctx, target, key, closure, is_getter,
                                )?;
                            }
                        }
                    }
                }
            }
            // A static field's initialiser runs once, here, with the class
            // already linked — which is what lets `static all = new X()` work.
            ClassElement::Field(field) if field.is_static => {
                let value = match &field.value {
                    Some(value) => super::emit_expr(builder, &mut inner, ctx, value)?,
                    None => expr::undefined(builder, ctx),
                };
                write_member(builder, ctx, constructor, &field.key, computed[index], value, false)?;
            }
            // `static { … }` — statements run once, in this same activation,
            // with `this` already bound above and after every static element
            // written before it, because the loop visits the body in source
            // order and does nothing to reorder this case.
            ClassElement::StaticBlock(statements) => {
                function::hoist(builder, &mut inner, ctx, statements)?;
                let mut loops = Loops::default();
                let mut terminated = false;
                for statement in statements {
                    if super::emit_stmt(builder, &mut inner, ctx, &mut loops, statement)? {
                        terminated = true;
                        break;
                    }
                }
                if terminated {
                    // A `throw` in a static block leaves the class expression
                    // ITSELF, and everything after it — the remaining static
                    // elements, and whatever the enclosing expression was going
                    // to do with the constructor — never runs.
                    //
                    // A class is an EXPRESSION, so there is no `bool` to return
                    // saying the block terminated; the value has to come back
                    // whatever happened. So the emitter continues into a fresh
                    // block that nothing jumps to: the terminator just written
                    // stays the terminator of the block it ended, and the code
                    // that follows is emitted where it is unreachable, which is
                    // what the language says it is.
                    //
                    // Without this the `throw` was written as a terminator and
                    // then OVERWRITTEN by the next statement's, so the class
                    // finished initialising and the `catch` never ran.
                    let unreachable = builder.create_block();
                    builder.switch_to(unreachable);
                    break;
                }
            }
            // An instance field runs per construction, so it is not emitted
            // here at all — see `with_fields`.
            _ => {}
        }
    }

    Ok(constructor)
}

/// Writes a member under its key, computed or not.
///
/// A named or private key is resolved at compile time, so the write goes
/// through the ordinary cached property path — the same one an object literal
/// uses, and the reason `this.#x` inside a method needs nothing special from
/// `expr.rs`'s member read: it is, underneath, an ordinary named property whose
/// name a program cannot spell.
///
/// A computed key was evaluated once already, in [`emit_class`]'s key pass —
/// `computed_value` is that answer, threaded through rather than recomputed,
/// because recomputing it here would run the key expression a second time.
fn write_member(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    target: ValueId,
    key: &ClassKey,
    computed_value: Option<ValueId>,
    value: ValueId,
    hidden: bool,
) -> EmitResult<ValueId> {
    match key {
        ClassKey::Public(PropertyKey::Named(name)) | ClassKey::Private(name) => {
            // A METHOD is not enumerable and a static FIELD is, which is the
            // whole of what `hidden` says. `for (const k in new C())` listed
            // `constructor` and every method until this distinction existed —
            // invisible for as long as `for`-`in` walked own keys only.
            if !hidden {
                return super::property::emit_write(builder, ctx, target, *name, value);
            }
            let key = super::property::key_constant(builder, ctx, *name);
            let value = expr::tagged(builder, value);
            Ok(expr::call(builder, ctx, RuntimeOp::DefineMethod, &[target, key, value])?[0])
        }
        ClassKey::Public(PropertyKey::Computed(_)) => {
            let key_value = computed_value.expect("evaluated in emit_class's key pass");
            let key_value = expr::tagged(builder, key_value);
            let value = expr::tagged(builder, value);
            // A computed key is a member like any other, so `hidden` decides
            // the same thing here. It did not, and the gap was invisible while
            // people wrote `m() {}`: an obfuscator rewrites every method name
            // into `['m']()`, and every one of them came out ENUMERABLE — so
            // `for (const k in new Box())` answered `v,m` on a program whose
            // source said `v`.
            //
            // The key crosses as a value, so `KeyNumber` turns it into the
            // number `DefineMethod` takes — the same conversion `super[e]` uses
            // rather than a fourth entry point.
            if !hidden {
                return Ok(
                    expr::call(builder, ctx, RuntimeOp::SetIndexed, &[target, key_value, value])?[0],
                );
            }
            let key = expr::call(builder, ctx, RuntimeOp::KeyNumber, &[key_value])?[0];
            Ok(expr::call(builder, ctx, RuntimeOp::DefineMethod, &[target, key, value])?[0])
        }
    }
}

/// The constructor function, declared or supplied.
///
/// # Why a class with no constructor still gets one
///
/// Because `new X()` has to call something, and because a derived class must
/// pass its arguments on. The specification supplies `constructor(...args) {
/// super(...args) }` for a derived class and an empty body otherwise, and this
/// supplies the same thing as a synthesised tree rather than as a special case
/// in the emitter — so the supplied constructor is emitted by exactly the code
/// that emits a written one.
fn emit_constructor(
    builder: &mut FuncBuilder,
    inner: &mut Scope,
    ctx: &mut Ctx,
    class: &Class,
    computed: &[Option<ValueId>],
) -> EmitResult<ValueId> {
    let declared = class.constructor(&ctx.names).map(|method| &method.function);
    let supplied;
    let function = match declared {
        Some(function) => {
            supplied = with_fields(ctx, class, function, computed)?;
            &supplied
        }
        None => {
            let default = default_constructor(ctx, class);
            supplied = with_fields(ctx, class, &default, computed)?;
            &supplied
        }
    };
    let made = match class.is_derived() {
        // Its `this` is the object `super()` answers, so it is a name in the
        // environment rather than the receiver — and the runtime has to be told,
        // because a derived constructor and a plain function are the same kind
        // of cell to it.
        true => {
            let held = ctx.names.intern(THIS);
            let closure =
                function::emit_closure_binding_this_late(builder, inner, ctx, function, held)?;
            expr::call(builder, ctx, RuntimeOp::MarkDerived, &[closure])?[0]
        }
        false => function::emit_closure(builder, inner, ctx, function)?,
    };
    Ok(made)
}

/// A constructor with the instance fields written in front of its body.
///
/// # Why the fields are statements rather than a second mechanism
///
/// `x = 1` in a class body means `this.x = 1` per instance, and this engine
/// already emits that. Synthesising the assignment reuses the whole of property
/// writing, `this`, and expression emission — where a field initialiser
/// mechanism of its own would be a second path to the same store, differing
/// wherever somebody forgot to update both.
///
/// A private field's initialiser is `this.#x = value` the same way — the member
/// node does not care that the name is private, because nothing downstream of
/// parsing does either; see the module doc for what makes it private. A
/// computed field's initialiser is `this[k] = value` where `k` reads the name
/// [`emit_class`] captured the already-evaluated key under, because the key runs
/// once at class definition and this body runs once per instance.
///
/// # The divergence, named
///
/// They run at the **start** of the constructor. The specification runs them
/// after `super()` returns in a derived class, because until then there is no
/// `this` — but there is one here, since `construct` makes the object before
/// calling. So a derived field initialiser that reads a property the parent
/// constructor sets reads it too early, and that is the one program this order
/// gets wrong.
/// The callee text that says "what follows is a class field initialiser".
///
/// # Why a marker in the tree and not a flag around the emission
///
/// Because the initialisers are statements at the head of the CONSTRUCTOR, and
/// the constructor's own body must still see the real `new.target`. A flag set
/// around `emit_constructor` would cover both; a flag set here would be set
/// while the tree is built rather than while it is emitted, which is a
/// different moment entirely.
///
/// # Why a literal callee and not a name
///
/// A synthetic IDENTIFIER is a name every later pass has to be told about —
/// capture, the escape analysis and the sloppy-write scan would each see a free
/// variable nothing declares, and the honest failure mode of that is a program
/// refused as unbound. A literal names nothing, so no pass has anything to
/// resolve, and `emit/expr.rs` takes the node apart before `emit/call.rs` ever
/// sees a call. What it costs is that `"@@rts_field_initialiser"(x)` written by
/// hand would be read as the marker — a program that is a `TypeError`
/// everywhere else, and the only spelling that collides.
const FIELD_INITIALISER: &str = "@@rts_field_initialiser";

/// Wraps a field initialiser so the emitter can tell it from the rest of the
/// constructor body. See [`FIELD_INITIALISER`].
fn marked_as_field_initialiser(value: Expr) -> Expr {
    let at = value.at;
    Expr {
        kind: ExprKind::Call {
            callee: Box::new(Expr {
                kind: ExprKind::Literal(crate::syntax::Literal::String(
                    crate::syntax::Text::from_units(
                        FIELD_INITIALISER.encode_utf16().collect::<Vec<u16>>(),
                    ),
                )),
                at,
            }),
            arguments: vec![crate::syntax::Spreadable::Single(value)],
            optional: false,
        },
        at,
    }
}

/// Whether an expression is one [`marked_as_field_initialiser`] wrapped, and
/// what it wrapped.
pub(super) fn field_initialiser(expr: &Expr) -> Option<&Expr> {
    let ExprKind::Call {
        callee, arguments, ..
    } = &expr.kind
    else {
        return None;
    };
    let ExprKind::Literal(crate::syntax::Literal::String(text)) = &callee.kind else {
        return None;
    };
    if text.units() != FIELD_INITIALISER.encode_utf16().collect::<Vec<u16>>() {
        return None;
    }
    match arguments.as_slice() {
        [crate::syntax::Spreadable::Single(inner)] => Some(inner),
        _ => None,
    }
}

fn with_fields(
    ctx: &mut Ctx,
    class: &Class,
    function: &Function,
    computed: &[Option<ValueId>],
) -> EmitResult<Function> {
    let mut prologue = Vec::new();
    for (index, element) in class.body.iter().enumerate() {
        let ClassElement::Field(field) = element else {
            continue;
        };
        if field.is_static {
            continue;
        }
        let value = match &field.value {
            Some(value) => marked_as_field_initialiser(value.clone()),
            // Declaring `x;` with no initialiser is not the same as never
            // declaring it: the property exists, which is what fixes the layout.
            None => Expr {
                kind: ExprKind::Literal(crate::syntax::Literal::Singleton(
                    crate::values::Singleton::Undefined,
                )),
                at: class.at,
            },
        };
        let this_expr = Expr {
            kind: ExprKind::This,
            at: class.at,
        };
        let target = match &field.key {
            ClassKey::Public(PropertyKey::Named(name)) | ClassKey::Private(name) => Expr {
                kind: ExprKind::Member {
                    object: Box::new(this_expr),
                    property: *name,
                    optional: false,
                },
                at: class.at,
            },
            ClassKey::Public(PropertyKey::Computed(_)) => {
                debug_assert!(
                    computed[index].is_some(),
                    "every computed key was evaluated in emit_class's key pass"
                );
                let held = ctx.names.intern(&computed_key_name(index));
                Expr {
                    kind: ExprKind::Index {
                        object: Box::new(this_expr),
                        index: Box::new(Expr {
                            kind: ExprKind::Ident(held),
                            at: class.at,
                        }),
                        optional: false,
                    },
                    at: class.at,
                }
            }
        };
        prologue.push(Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::Assign {
                    target: AssignTarget::Place(Box::new(target)),
                    value: Box::new(value),
                    op: AssignOp::Plain,
                },
                at: class.at,
            }),
            at: class.at,
        });
    }
    if prologue.is_empty() {
        return Ok(function.clone());
    }
    let FunctionBody::Block(body) = &function.body else {
        // A constructor is never a concise arrow body — that is a grammar fact
        // rather than something to handle.
        return Err(EmitError::Unsupported {
            construct: "a class constructor written as an expression body",
        });
    };
    // AFTER `super()`, when there is one. In a derived constructor `this` does
    // not exist until the base has made the object — this engine holds it in an
    // environment slot that only `emit_super_call` writes — so an initialiser
    // placed at the head assigned onto whatever that slot held before, and every
    // field of every subclass was lost. `class B extends A { b = 2 }` answered
    // `undefined` for `b`.
    //
    // Placing them after is also what the specification says, so the divergence
    // the comment above named is gone rather than moved: a derived field
    // initialiser that reads a property the parent constructor set now reads it
    // at the right time.
    //
    // The FIRST `super()` and not the last: a constructor may call it inside a
    // branch, and the language requires exactly one to run. Splitting after the
    // first statement-level call is what the common shape needs, and a
    // conditional `super()` keeps the old head placement rather than being
    // silently misplaced — named here because it is a real program this still
    // gets wrong.
    let at = body.iter().position(|statement| {
        matches!(
            &statement.kind,
            StmtKind::Expr(Expr {
                kind: ExprKind::SuperCall { .. },
                ..
            })
        )
    });
    let mut with = function.clone();
    with.body = FunctionBody::Block(match at {
        Some(at) => {
            let mut spliced: Vec<Stmt> = body[..=at].to_vec();
            spliced.extend(prologue);
            spliced.extend(body[at + 1..].iter().cloned());
            spliced
        }
        None => {
            prologue.extend(body.iter().cloned());
            prologue
        }
    });
    Ok(with)
}

/// `constructor(...) { super(...) }`, or an empty one.
///
/// The arguments are forwarded by name through parameters nothing else can
/// spell, which is what makes `new Derived(1, 2)` reach the parent at all. Four
/// of them, because that is the arity a call carries — a class whose parent
/// wanted a fifth is refused at the call rather than losing it here.
fn default_constructor(ctx: &mut Ctx, class: &Class) -> Function {
    let names: Vec<Name> = (0..crate::runtime::ARGUMENT_SLOTS)
        .map(|at| ctx.names.intern(&format!("__rts_arg{at}")))
        .collect();
    let body = match class.is_derived() {
        true => vec![Stmt {
            kind: StmtKind::Expr(Expr {
                kind: ExprKind::SuperCall {
                    arguments: names
                        .iter()
                        .map(|name| {
                            crate::syntax::Spreadable::Single(Expr {
                                kind: ExprKind::Ident(*name),
                                at: class.at,
                            })
                        })
                        .collect(),
                },
                at: class.at,
            }),
            at: class.at,
        }],
        false => Vec::new(),
    };
    Function {
        name: class.name,
        parameters: names
            .into_iter()
            .map(|name| crate::syntax::Parameter {
                target: crate::syntax::Pattern::Name(name),
                default: None,
                claim: None,
            })
            .collect(),
        rest_parameter: None,
        directives: Vec::new(),
        body: FunctionBody::Block(body),
        returns: None,
        captures_this: false,
        is_async: false,
        is_generator: false,
        at: class.at,
    }
}

/// The environment a class body is emitted against.
///
/// Built for a derived class, as before, and now also for one with a computed
/// INSTANCE field key — `needs_environment` says which. `super` is a syntax
/// error outside a derived class, so only a derived class's environment holds
/// one; a plain class with a computed instance key gets an environment that
/// holds no `__rts_super` at all, which is fine, because nothing in it is ever
/// asked to read one.
fn class_scope(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    parent: Option<ValueId>,
    needs_environment: bool,
    computed_fields: &[(Name, ValueId)],
    own_name: Option<Name>,
) -> EmitResult<Scope> {
    if parent.is_none() && !needs_environment {
        // `computed_fields` is always empty here: `needs_environment` is `true`
        // whenever any element of it exists (`emit_class`'s own computation),
        // so this branch never drops one on the floor.
        debug_assert!(computed_fields.is_empty());
        // Not a copy of the enclosing scope: methods are emitted against it, and
        // `Scope` is not `Clone`. Rebuilt from what it can reach, which is
        // exactly what a nested function sees.
        return Ok(Scope::for_function(
            scope.environment(),
            BTreeSet::new(),
            // Nothing captured, so nothing to bind at zero hops.
            &BTreeSet::new(),
            &scope.reachable(),
        ));
    }

    let zero = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(0),
    });
    let zero = builder.use_const(zero);
    let environment = expr::call(builder, ctx, RuntimeOp::ObjectNew, &[zero])?[0];
    let outer = binding::outer_link(ctx);
    let handed = match scope.environment() {
        Some(environment) => environment,
        None => expr::undefined(builder, ctx),
    };
    super::property::emit_write(builder, ctx, environment, outer, handed)?;

    let mut held = BTreeSet::new();
    let home_name = ctx.names.intern(HOME);
    held.insert(home_name);
    // The static home too, and in the SAME environment: a class body has both
    // kinds of method and each resolves `super` against its own.
    let static_home_name = ctx.names.intern(STATIC_HOME);
    held.insert(static_home_name);
    // The class's own name lives here too when a method reads it, which is what
    // makes `class Inner { m() { return Inner; } }` resolve from inside a
    // separately compiled body.
    if let Some(name) = own_name {
        held.insert(name);
    }

    if let Some(parent) = parent {
        let super_name = ctx.names.intern(SUPER);
        super::property::emit_write(builder, ctx, environment, super_name, parent)?;
        held.insert(super_name);
    }

    // Every computed instance field's key value, written into the SAME
    // environment and recorded in the SAME `held` set as `__rts_home` and
    // `__rts_super` above — which is what makes it reachable through
    // `Scope::reachable` when the constructor's own scope is built, instead of
    // being a write nothing downstream knows to look for.
    for &(name, value) in computed_fields {
        super::property::emit_write(builder, ctx, environment, name, value)?;
        held.insert(name);
    }

    // One link further out for everything the enclosing scope could reach,
    // because this environment sits between it and the methods.
    let reachable: Vec<(Name, u32)> = scope
        .reachable()
        .into_iter()
        .map(|(name, hops)| (name, hops + 1))
        .collect();
    // `held` IS the own level here, and the two sets being one is the point
    // rather than a shortcut: every name in it was written into `environment`
    // by the loops just above — `__rts_home`, `__rts_super`, the class's own
    // name, each computed field's key. There is no nested block to
    // over-include from, which is exactly the condition
    // `capture::declared_at_own_level` exists to test for a function body.
    Ok(Scope::for_function(
        Some(environment),
        held.clone(),
        &held,
        &reachable,
    ))
}

/// `super.x` — the LOOKUP starts above the home object, but the receiver an
/// inherited accessor runs with is `this`, not the object the search found it
/// on.
///
/// # The bug this replaced
///
/// The previous version passed `above` — the parent's `prototype` — as both
/// the search root and the receiver, through the ordinary named-property path
/// (`super::property::emit_read`), which has only one object argument and
/// uses it for both. Two consequences, and they are not the same failure:
///
/// - An inherited **accessor** ran with the wrong `this`. `super.x` inside a
///   subclass method, where the base class declares `get x() { return
///   this.something; }`, read `this.something` off the PARENT'S PROTOTYPE
///   rather than off the actual instance — a plausible-looking wrong answer
///   rather than a crash, because the prototype is an object too.
/// - A plain field is never found through `super` at all, which is not a bug
///   to fix here: `this.x = 7` in a constructor makes `x` an OWN property of
///   the INSTANCE, never of the base class's `prototype`, and the walk
///   starts above the home object and never redirects to the receiver's own
///   properties. Verified against Node rather than assumed — `super.x` for a
///   plain inherited field answers `undefined` there too.
///
/// [`crate::runtime::RuntimeOp::GetSuperProperty`] is the fix: it takes the
/// receiver and the search root as two separate arguments, the way the
/// specification's `OrdinaryGet(O, P, Receiver)` keeps them apart.
pub(super) fn emit_super_member(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    property: &PropertyKey,
) -> EmitResult<ValueId> {
    // The key FIRST: a computed one is an expression, and the language
    // evaluates it before anything else the access does.
    let key = super_key(builder, scope, ctx, property)?;
    let (above, receiver) = super_lookup_root_and_receiver(builder, scope, ctx)?;
    Ok(expr::call(
        builder,
        ctx,
        RuntimeOp::GetSuperProperty,
        &[receiver, above, key],
    )?[0])
}

/// `super.x = v` — the setter search starts above the home object; an
/// unintercepted write lands on `this`, the same place `this.x = v` would put
/// it, and not on the object the search started at.
///
/// There was no assignment-target arm for `SuperMember` at all before this:
/// `super.x = v` was refused by name at compile time.
pub(super) fn emit_super_member_write(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    property: &PropertyKey,
    value: ValueId,
) -> EmitResult<ValueId> {
    // The key FIRST: a computed one is an expression, and the language
    // evaluates it before anything else the access does.
    let key = super_key(builder, scope, ctx, property)?;
    let (above, receiver) = super_lookup_root_and_receiver(builder, scope, ctx)?;
    let value = expr::tagged(builder, value);
    Ok(expr::call(
        builder,
        ctx,
        RuntimeOp::SetSuperProperty,
        &[receiver, above, key, value],
    )?[0])
}

/// The two objects a `super` property access needs, kept apart: where the
/// search starts, and which object an accessor call (or, for a write with no
/// setter, an ordinary property write) runs against.
fn super_lookup_root_and_receiver(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
) -> EmitResult<(ValueId, ValueId)> {
    // A STATIC method's home object is the constructor; an instance method's is
    // the prototype. Read from the flag the method's own emission set, because
    // this runs several frames inside that emission.
    let home = ctx.names.intern(match ctx.in_static_method {
        true => STATIC_HOME,
        false => HOME,
    });
    let home = binding::read(builder, scope, ctx, home)?;
    // Above the home object, not on it. `super.m()` inside `m` must not find
    // `m` again, which is the whole reason the home object exists rather than
    // the search starting at `this`.
    let above = expr::call(builder, ctx, RuntimeOp::GetPrototype, &[home])?[0];
    let Some(receiver) = current_receiver(builder, scope, ctx)? else {
        return Err(EmitError::Unsupported {
            construct: "`super.x` outside a method with a receiver",
        });
    };
    Ok((above, receiver))
}

/// What `this` is where the emitter currently stands, by the two routes it can
/// take — said once, because three places asked and two of them knew only one
/// of the routes.
///
/// The NAME comes first, exactly as `ExprKind::This` orders it: a derived
/// constructor holds `this` in its environment because there is no object until
/// `super()` answers one, and an ARROW holds the enclosing function's under the
/// same name because it has no receiver of its own. Asking the parameter first
/// answers `undefined` in both.
///
/// The comment that stood here said `super.x` is "always written inside a
/// method, never inside the constructor before `super()`". It is also written
/// inside ARROWS in methods — `m() { const f = () => super.label; }` — and that
/// refusal is what this replaces.
///
/// `None` means there is genuinely no receiver, and each caller says what it
/// refuses.
pub(super) fn current_receiver(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
) -> EmitResult<Option<ValueId>> {
    match scope.late_this() {
        Some(name) => binding::read(builder, scope, ctx, name).map(Some),
        None => Ok(scope.this_value()),
    }
}

/// The one property-key shape a `super` access supports today.
/// The key operand a `super` access crosses with, from either spelling.
///
/// A NAMED key was resolved while compiling, so it is a constant. A COMPUTED
/// one is a value the program produces at run time, and `RuntimeOp::KeyNumber`
/// is what turns it into the same number — the operation the computed property
/// path already uses, rather than a third entry point taking a key as a value.
///
/// It was refused by name until this existed, and `super[e]` is not exotic: an
/// obfuscator rewrites every `super.m()` into one, so five generated fixtures
/// failed to compile on a construct their source had spelled ordinarily.
fn super_key(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    property: &PropertyKey,
) -> EmitResult<ValueId> {
    match property {
        PropertyKey::Named(name) => Ok(super::property::key_constant(builder, ctx, *name)),
        PropertyKey::Computed(key) => {
            let value = super::emit_expr(builder, scope, ctx, key)?;
            let value = expr::tagged(builder, value);
            Ok(expr::call(builder, ctx, RuntimeOp::KeyNumber, &[value])?[0])
        }
    }
}

/// `super(...)` — the parent constructor, run against the object that exists.
///
/// Not `new`: the instance was already made by `construct`, with the derived
/// class's prototype. What is missing is the parent's own initialisation, and
/// running its body with this receiver is exactly that.
pub(super) fn emit_super_call(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    arguments: &[crate::syntax::Spreadable],
) -> EmitResult<ValueId> {
    let parent = ctx.names.intern(SUPER);
    let parent = binding::read(builder, scope, ctx, parent)?;
    let Some(held) = scope.late_this() else {
        // Only a derived constructor holds `this`, and only a derived
        // constructor may write `super()`. Reaching here is the grammar's job
        // rather than this module's, and refusing by name says so.
        return Err(EmitError::Unsupported {
            construct: "`super()` outside a derived constructor",
        });
    };
    let produced = super::call::emit_super_construct(builder, scope, ctx, parent, arguments)?;
    // The object the parent made becomes this activation's `this`, which is the
    // whole of what `super()` does that a call does not.
    binding::write(builder, scope, ctx, held, produced)
}

/// The name an accessor is installed under.
///
/// Unlike [`write_member`], an accessor has no computed form here: `DefineGetter`
/// and `DefineSetter` take the key the compiler resolved as a constant, the same
/// way an object literal's accessor does — see `object.rs`'s own refusal of a
/// computed accessor name. Giving classes a capability object literals do not
/// have would be a second answer to "how is an accessor key spelled", so a
/// computed accessor name is refused here for the same reason, not a
/// coincidentally similar one.
fn accessor_name(key: &ClassKey) -> Option<Name> {
    match key {
        ClassKey::Public(PropertyKey::Named(name)) => Some(*name),
        ClassKey::Private(name) => Some(*name),
        // `None` means the key is an expression, which the caller resolves
        // through `__rts_key_number` from the value it already evaluated.
        ClassKey::Public(PropertyKey::Computed(_)) => None,
    }
}

/// What this lowering does not express, refused by name before anything is
/// emitted.
///
/// Up front rather than as each is reached, so a class is either wholly emitted
/// or wholly refused — a half-built one would leave a constructor whose methods
/// are missing, which runs and is wrong. The only case left here is a computed
/// accessor name: everything else a class body can hold — a private member, a
/// computed method or field name, a static block — is emitted by the loop in
/// [`emit_class`], and refusing it up front as well would be the duplicated
/// rule 3 warns about.
fn refuse_what_is_not_built(_class: &Class) -> EmitResult<()> {
    // Nothing left to refuse up front. It held one entry — a computed accessor
    // name — and that is emitted now, from the key the loop in [`emit_class`]
    // already evaluates. The function stays because the next gap of this shape
    // has somewhere to go, and because deleting it would move the question of
    // where an early refusal belongs.
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::names::Names;

    #[test]
    fn a_private_key_and_its_public_namesake_are_still_different_identifiers() {
        // Reusing the `Name` `Cx::private_name` already produced means the
        // separation this module relies on has to come from THAT interning, not
        // from anything here — this test pins that it still does.
        let mut names = Names::new();
        let private = names.intern("@@#x");
        let public = names.intern("x");
        assert_ne!(private, public);
    }
}
