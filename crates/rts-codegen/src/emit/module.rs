//! `import`, as statements the body already knows how to run.
//!
//! # Why an import becomes a declaration
//!
//! Because that is what it is, everywhere except in when it happens.
//! `import { test } from "rts:test"` introduces `test` in the module's scope,
//! and the only thing that distinguishes it from `const test = …` is that the
//! right-hand side is not written in the program.
//!
//! So this synthesises the missing right-hand side — a call to the runtime
//! carrying which specifier and which name — and hands the declarations to the
//! ordinary body emitter. Nothing downstream learns what a module is: a
//! captured import is captured by the machinery that captures a `const`, and a
//! read of one is a read of a local.
//!
//! # What this does NOT do
//!
//! Resolve a specifier, read a file, order an evaluation, or link a cycle. The
//! runtime answers a specifier from a table the host filled, so `"rts:test"`
//! works and `"./other.ts"` answers `undefined` — visibly, rather than by being
//! quietly bound to something else. A real module system is the thing that
//! replaces this, and it will replace the runtime side rather than this file:
//! what an import MEANS for a scope is decided here and does not change.
//!
//! # Why `export` is refused rather than ignored
//!
//! An ignored `export` compiles a program whose exports nobody can see, which
//! looks like it worked. The suite this engine is measured against exports
//! nothing, so refusing costs nothing there and keeps the gap visible.

use rts_cranelift::ir::{ConstDecl, FuncBuilder, ScalarBits, ValueId};
use rts_cranelift::repr::Repr;

use super::{Ctx, EmitError, EmitResult, Scope};
use crate::runtime::RuntimeOp;
use crate::syntax::{Import, ImportBinding};

/// Binds everything one `import` introduces.
///
/// Emitted into the body's own scope, in source order among the statements —
/// which is not where the specification hoists them, and is the divergence to
/// state: a module's imports are all bound before any of its statements run, so
/// a program reading an imported name ABOVE the `import` line sees `undefined`
/// here and the value there. Every file in the corpus this serves writes its
/// imports first.
pub fn emit_import(
    builder: &mut FuncBuilder,
    scope: &mut Scope,
    ctx: &mut Ctx,
    import: &Import,
) -> EmitResult<()> {
    // The specifier is a string the program wrote, so it is already in the
    // literal table — the same table `string_const` reads, and the number is
    // what crosses rather than the text.
    let specifier = ctx.literal(&import.source);
    let specifier = number(builder, u64::from(specifier));

    for binding in &import.bindings {
        let (local, value) = match binding {
            ImportBinding::Named { exported, local } => {
                let name = ctx.names.intern(exported);
                let key = number(builder, u64::from(ctx.key_of(name)));
                let read = super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::ModuleBinding,
                    &[specifier, key],
                )?[0];
                (*local, read)
            }
            // `import d from "m"` is `import { default as d }`, which is what
            // the specification says it is — and saying it here rather than in
            // the runtime keeps "default" a name in the key registry like any
            // other rather than a case the lookup knows about.
            ImportBinding::Default(local) => {
                let name = ctx.names.intern("default");
                let key = number(builder, u64::from(ctx.key_of(name)));
                let read = super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::ModuleBinding,
                    &[specifier, key],
                )?[0];
                (*local, read)
            }
            ImportBinding::Namespace(local) => {
                let read = super::expr::call(
                    builder,
                    ctx,
                    RuntimeOp::ModuleNamespace,
                    &[specifier],
                )?[0];
                (*local, read)
            }
        };
        super::binding::declare(builder, scope, ctx, local, value)?;
    }
    Ok(())
}

/// Refuses an `export`, by name.
pub fn emit_export() -> EmitResult<()> {
    Err(EmitError::Unsupported {
        construct: "an export",
    })
}

/// An integer operand: which specifier, or which key.
///
/// Not a tagged value, for the reason a property key is not one: neither is
/// something the program could compute, and emitting one tagged would claim it
/// could.
fn number(builder: &mut FuncBuilder, bits: u64) -> ValueId {
    let id = builder.declare_const(ConstDecl::Scalar {
        repr: Repr::I64,
        bits: ScalarBits(bits),
    });
    builder.use_const(id)
}
