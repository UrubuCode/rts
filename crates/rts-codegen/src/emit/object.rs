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

pub(super) fn define_accessor(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    object: ValueId,
    name: Name,
    function: ValueId,
    is_getter: bool,
) -> EmitResult<ValueId> {
    let key = key_constant(builder, ctx, name);
    let function = tagged(builder, function);
    let op = match is_getter {
        true => RuntimeOp::DefineGetter,
        false => RuntimeOp::DefineSetter,
    };
    Ok(call(builder, ctx, op, &[object, key, function])?[0])
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
    for property in properties {
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
                let value = super::function::emit_closure_method(builder, scope, ctx, function)?;
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
                let closure = super::function::emit_closure_method(builder, scope, ctx, function)?;
                match key {
                    PropertyKey::Named(name) => {
                        define_accessor(builder, ctx, object, *name, closure, is_getter)?;
                    }
                    PropertyKey::Computed(expr) => {
                        let key = super::expr::emit_expr(builder, scope, ctx, expr)?;
                        define_computed_accessor(builder, ctx, object, key, closure, is_getter)?;
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
            PropertyKey::Named(name) => {
                super::property::emit_write(builder, ctx, object, *name, value)?;
            }
            PropertyKey::Computed(expression) => {
                let key = emit_expr(builder, scope, ctx, expression)?;
                let key = tagged(builder, key);
                call(builder, ctx, RuntimeOp::SetIndexed, &[object, key, value])?;
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
    Ok(call(builder, ctx, op, &[object, key, function])?[0])
}

