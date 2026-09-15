//! An object literal, and the accessors a literal or a class body defines.
//!
//! # Why an accessor is defined and not written
//!
//! A getter stored under its key would be **returned** by a cached read rather
//! than called: compiled code emits `cached_get`, which loads the slot a layout
//! says the key is at. So the pair lives beside the cell in the runtime, out of
//! the layout entirely, and defining one is its own operation.
//!
//! That is the mirror of the decision an array's `length` needed. `length`
//! became a real property so the fast path and the runtime would agree; an
//! accessor must not be one, so that the fast path misses and the read reaches
//! the runtime at all.
//!
//! # Why this is not in `expr.rs`
//!
//! Because that file is at the thousand-line ceiling rule 8 sets, and because
//! the literal and the class body both define accessors — putting the shared
//! half here is what stops the two spellings from disagreeing about where the
//! pair is kept.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::expr::{call, emit_expr, tagged};
use super::property::key_constant;
use super::{Ctx, EmitResult, Scope};
use crate::names::Name;
use crate::runtime::RuntimeOp;
use crate::syntax::{Property, PropertyKey};

/// Emits one half of an accessor definition.
///
/// Shared by the object literal and the class body, which spell the same thing
/// two ways and must not disagree about it — the pair is kept beside the cell,
/// deliberately absent from the layout, and a second copy of that decision is
/// where one of them would write a property instead.
/// The name `SetFunctionName` gives an accessor: the key with `get ` or `set `
/// in front of it.
///
/// Interned rather than carried as a string, because `Ctx::lend_name` takes a
/// `Name` — and interning is what makes the prefixed spelling one name in the
/// table rather than a second string per accessor emitted.
pub(super) fn accessor_name(
    ctx: &mut super::Ctx,
    key: crate::names::Name,
    is_getter: bool,
) -> crate::names::Name {
    let prefix = match is_getter {
        true => "get ",
        false => "set ",
    };
    let spelled = format!("{prefix}{}", ctx.names.text(key));
    ctx.names.intern(&spelled)
}

/// `enumerable` is the caller's, and the two callers disagree on purpose: an
/// object literal's accessor is enumerable and a class body's is not, which is
/// the same split `DefineMethod` draws for an ordinary member. Both passed
/// nothing before, and the runtime recorded nothing, so a class's accessor
/// appeared in `Object.keys(C.prototype)`.
pub(super) fn define_accessor(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    object: ValueId,
    name: Name,
    function: ValueId,
    is_getter: bool,
    enumerable: bool,
) -> EmitResult<ValueId> {
    let key = key_constant(builder, ctx, name);
    let function = tagged(builder, function);
    let op = match is_getter {
        true => RuntimeOp::DefineGetter,
        false => RuntimeOp::DefineSetter,
    };
    // `builder.bool_constant` and NOT `expr::boolean_constant`, which builds a
    // TAGGED boolean value. The entry point takes a Rust `bool`, so its declared
    // shape is `Repr::Bool`, and a tagged word there is refused by the emitter
    // — `CallArgumentRepr { expected: Bool, found: Tagged }`, on every program
    // that defines an accessor, which is most of them.
    let flag = builder.bool_constant(enumerable);
    Ok(call(builder, ctx, op, &[object, key, function, flag])?[0])
}


/// Emits an object literal.
///
/// A fresh object, then one write per property, in source order. Not a shape
/// decided here and filled in: two objects built the same way reach the same
/// layout because they take the same transitions, and taking them is what the
/// writes do. Deciding a layout at the literal would be a second authority on
/// what an object's shape is, disagreeing with the runtime's the first time a
/// property was added after construction.
pub(super) fn emit_object(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    properties: &[Property],
) -> EmitResult<ValueId> {
    // How many slots this literal will need, so the runtime can take a cell
    // wide enough in one go. A hint and not a shape: the writes below still
    // decide the layout, and a wrong count costs a slot rather than an answer.
    let expected = builder.declare_const(rts_cranelift::ir::ConstDecl::Scalar {
        repr: rts_cranelift::repr::Repr::I64,
        bits: rts_cranelift::ir::ScalarBits(properties.len() as u64),
    });
    let expected = builder.use_const(expected);
    let object = call(builder, ctx, RuntimeOp::ObjectNew, &[expected])?[0];
    // A method of this literal may write `super`, and the object it resolves
    // against is THIS one — a literal has a `[[HomeObject]]` exactly as a class
    // body does. Built here, before any method is emitted, because the method
    // bodies are separately compiled functions that reach the name through the
    // environment chain and nothing else.
    //
    // Only when a method actually reaches `super`: the environment is an
    // allocation and two property writes, and the overwhelmingly common literal
    // with methods writes no `super` at all. See `home::reaches_super` for
    // which direction that question over-approximates in.
    let home = match properties.iter().any(|property| match property {
        Property::Method { function, .. }
        | Property::Getter { function, .. }
        | Property::Setter { function, .. } => super::home::reaches_super(function),
        _ => false,
    }) {
        true => {
            let name = ctx.names.intern(super::home::HOME);
            Some(super::home::environment_holding(
                builder,
                scope,
                ctx,
                &[(name, Some(object))],
            )?)
        }
        false => None,
    };
    for property in properties {
        // A COMPUTED key is evaluated BEFORE the value it names, which is the
        // order `PropertyDefinitionEvaluation` states and the order this had
        // backwards: the value was emitted in the match below and the key only
        // at the write, so `{ [k()]: v() }` ran `v` first. Invisible until
        // either side has an effect, and then it is the whole answer —
        // `{ [k("ka")]: v("v1"), b: v("v2"), [k("kb")]: v("v3") }` logged
        // `v1,ka,v2,v3,kb` where every runtime logs `ka,v1,v2,kb,v3`.
        //
        // Hoisted here rather than fixed at each of the three arms, because it
        // is one rule about the property and not three about its shapes: a
        // value, a method and an accessor all name themselves the same way.
        let computed_key = match property {
            Property::Value {
                key: PropertyKey::Computed(expression),
                ..
            }
            | Property::Method {
                key: PropertyKey::Computed(expression),
                ..
            }
            | Property::Getter {
                key: PropertyKey::Computed(expression),
                ..
            }
            | Property::Setter {
                key: PropertyKey::Computed(expression),
                ..
            } => Some(emit_expr(builder, scope, ctx, expression)?),
            _ => None,
        };
        // A method is a function stored under a key, plus a **home object** —
        // which is what `super.x` inside it reads from. There is no `super`
        // yet, so the two are the same thing here; when there is, this becomes
        // the place that differs and the tree already records which was
        // written.
        let (key, value) = match property {
            Property::Value { key, value, .. } => {
                // NamedEvaluation, the same rule `const f = function () {}`
                // gets: `{ gamma: function () {} }` names that function
                // `gamma`. It was done only at a declaration, so every function
                // reached through an object literal — which is most of them in
                // a module that exports a table of handlers — had an empty
                // `.name`, and `bind` inherits the emptiness on top of that.
                //
                // A COMPUTED key lends nothing: the name would be whatever the
                // expression evaluates to, which is a runtime string this
                // emitter does not have.
                //
                // And only an ANONYMOUS definition is named: `{ gamma: named }`
                // must keep the name the function already has, so the guard is
                // what makes lending safe rather than merely convenient.
                if let PropertyKey::Named(name) = key
                    && super::stmt::anonymous_definition(value)
                {
                    ctx.lend_name(*name);
                }
                let value = emit_expr(builder, scope, ctx, value)?;
                // Anything the initialiser did not take is dropped: a lent name
                // that outlived its initialiser would be picked up by the next
                // function emitted anywhere, which is a wrong name rather than a
                // missing one.
                let _ = ctx.take_lent_name();
                (key, value)
            }
            Property::Method { key, function } => {
                // A method has no name of its own in the tree — `{ beta() {} }`
                // parses as a function with no identifier — so the key is the
                // only thing that can name it, and the language says it does.
                if let PropertyKey::Named(name) = key {
                    ctx.lend_name(*name);
                }
                let value = match &home {
                    Some(inner) => {
                        super::function::emit_closure_method(builder, inner, ctx, function)?
                    }
                    None => super::function::emit_closure_method(builder, scope, ctx, function)?,
                };
                let _ = ctx.take_lent_name();
                (key, value)
            }
            // An accessor is defined, not written: a getter stored under the
            // key would be returned by a cached read rather than called. The
            // computed spelling is refused because the definition takes the key
            // the compiler resolved, and a value would need a second entry
            // point that interns one.
            Property::Getter { key, function } | Property::Setter { key, function } => {
                let is_getter = matches!(property, Property::Getter { .. });
                // `SetFunctionName` with a PREFIX: the specification names a
                // getter `"get width"` and a setter `"set width"`, which is how
                // a stack trace tells the two halves of one property apart.
                // Both answered `""`, since an accessor's function has no
                // identifier of its own in the tree.
                if let PropertyKey::Named(name) = key {
                    let spelled = accessor_name(ctx, *name, is_getter);
                    ctx.lend_name(spelled);
                }
                let closure = match &home {
                    Some(inner) => {
                        super::function::emit_closure_method(builder, inner, ctx, function)?
                    }
                    None => super::function::emit_closure_method(builder, scope, ctx, function)?,
                };
                match key {
                    PropertyKey::Named(name) => {
                        // ENUMERABLE: an object literal's accessor is, and this
                        // is the caller that says so — see `define_accessor`.
                        define_accessor(builder, ctx, object, *name, closure, is_getter, true)?;
                    }
                    PropertyKey::Computed(_) => {
                        // Already evaluated, above the closure — the key comes
                        // first for an accessor as much as for a value.
                        let key = computed_key.expect("a computed accessor key");
                        define_computed_accessor(
                            builder, ctx, object, key, closure, is_getter, true,
                        )?;
                    }
                }
                continue;
            }
            // `{ ...source }` — the source's own enumerable properties, copied
            // in the order they are written. A getter on the source RUNS and
            // what lands is a plain data property, which is what the language
            // says a spread does and the difference from inheriting.
            Property::Spread(source) => {
                let source = emit_expr(builder, scope, ctx, source)?;
                let source = super::expr::as_value(builder, source);
                super::expr::call(builder, ctx, RuntimeOp::ObjectSpread, &[object, source])?;
                continue;
            }
            // `__proto__: v` in a literal SETS the prototype rather than adding
            // a property, and only in this spelling — the tree already made that
            // distinction, so nothing here has to re-decide it.
            Property::Prototype(value) => {
                let value = emit_expr(builder, scope, ctx, value)?;
                let value = super::expr::as_value(builder, value);
                super::expr::call(builder, ctx, RuntimeOp::SetPrototype, &[object, value])?;
                continue;
            }
        };
        let value = tagged(builder, value);

        match key {
            // The name is resolved while compiling, so the key crosses as the
            // number — which is the whole reason a written key and a computed
            // one are different operations rather than one taking a value.
            // Through the CACHED store, which is the same terminator a class
            // constructor's `this.x = a` uses — and that is the whole of this
            // change. A raw `SetProperty` call takes the shape transition
            // fresh every time; the cached store remembers it, so the second
            // object built by this site is born knowing where its fields go.
            //
            // Measured before, 300 000 objects that escape: `new P(1, 2)` cost
            // 40 ms and `{x: 1, y: 2}` cost 143 — the CLASS was three times
            // faster than the literal, and this was why. It scaled with the
            // field count: 83 ms for one field, 143 for two, 206 for three.
            // DEFINED and not written. `PropertyDefinitionEvaluation` is
            // `CreateDataPropertyOrThrow`, which is `[[DefineOwnProperty]]` —
            // it never consults an accessor. A `[[Set]]` does, and the literal
            // is where one can already be in the way: `{ get z() {…}, z: 5 }`
            // is a legal literal whose second property REPLACES the accessor,
            // and writing it threw `TypeError: … which has only a getter`. The
            // same is true of any accessor the literal's own `__proto__:` put
            // on the chain before the write.
            //
            // It costs nothing: `emit_define` and `emit_write` share the one
            // `cached_set` fast path and differ only in the entry point the
            // MISS calls, so the measurement in the comment above still holds.
            PropertyKey::Named(name) => {
                super::property::emit_define(builder, ctx, object, *name, value)?;
            }
            // DEFINED, exactly as the named arm above is, and for the reasons
            // written there. This was a `[[Set]]`, which is the one difference
            // the two arms are not allowed to have — `{ get z() {…}, [k]: 5 }`
            // with `k` of `"z"` ran the getter's setter instead of replacing
            // the accessor, and `{ ["__proto__"]: v }` reached `Object.
            // prototype`'s `__proto__` SETTER and changed the object's
            // prototype. The language says the computed spelling is the one
            // that does not: only a literal (or quoted) `__proto__:` is the
            // prototype setter, and every other spelling is an ordinary own
            // property of that name.
            //
            // `DefineField` is the define primitive `docs/codegen/object-
            // model.md` prescribes, and that document already names an object
            // literal's property as its third site.
            PropertyKey::Computed(_) => {
                let key = computed_key.expect("a computed property key");
                let key = tagged(builder, key);
                let key = call(builder, ctx, RuntimeOp::KeyNumber, &[key])?[0];
                call(builder, ctx, RuntimeOp::DefineField, &[object, key, value])?;
            }
        }
    }
    Ok(object)
}

/// The same, for an accessor whose key is an expression.
///
/// `{ get [e]() {} }` and `class C { get [e]() {} }` were both refused, and the
/// reason given was that the definition takes the key the COMPILER resolved
/// while a computed one has a value. That is still true — what changed is that
/// the value can be resolved: `__rts_key_number` answers the number, and the
/// pair that already exists takes it from there.
///
/// So there is one way to define an accessor and not two, which is what a second
/// entry point taking a value would have made.
pub(super) fn define_computed_accessor(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    object: ValueId,
    key: ValueId,
    function: ValueId,
    is_getter: bool,
    enumerable: bool,
) -> EmitResult<ValueId> {
    // The key arrives as a VALUE the caller already produced, because a class
    // body evaluates every computed key once before installing anything —
    // re-emitting the expression here would evaluate it a second time, and a key
    // written `[next()]` would advance twice.
    let key = tagged(builder, key);
    let key = call(builder, ctx, RuntimeOp::KeyNumber, &[key])?[0];
    let function = tagged(builder, function);
    let op = match is_getter {
        true => RuntimeOp::DefineGetter,
        false => RuntimeOp::DefineSetter,
    };
    // `builder.bool_constant` and NOT `expr::boolean_constant`, which builds a
    // TAGGED boolean value. The entry point takes a Rust `bool`, so its declared
    // shape is `Repr::Bool`, and a tagged word there is refused by the emitter
    // — `CallArgumentRepr { expected: Bool, found: Tagged }`, on every program
    // that defines an accessor, which is most of them.
    let flag = builder.bool_constant(enumerable);
    Ok(call(builder, ctx, op, &[object, key, function, flag])?[0])
}

