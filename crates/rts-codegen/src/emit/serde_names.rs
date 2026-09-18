//! Telling the pickle which classes and functions a program declares.
//!
//! `rts:serde` writes a class instance as the name of its class and a function
//! as its own name, and reads each back by looking the name up in the program
//! that decodes it. At run time a class is a constructor function (`class.rs`
//! says why there is no class in the runtime), and nothing about a function
//! says which file declared it or whether it was declared at the top level —
//! so the compiler, which knows both, says so where each is declared, through
//! `RuntimeOp::SerdeDeclare`.
//!
//! # What is registered
//!
//! Every NAMED class, wherever it is written — a class inside a function is
//! registered each time the function runs, replacing its own entry, which is
//! what bounds the registry by the source text.
//!
//! Code compiled by `eval`, `new Function` or a page `<script>` registers
//! nothing: [`super::Ctx::module_key`] is `None` there, and a name that only
//! exists while some other code is running is not one a file can refer to.
//!
//! # The module key
//!
//! A class is named by its module and its name, so two `class Foo` in two
//! files are two classes. The module is its path RELATIVE TO THE ENTRY — `""`
//! for the entry itself, `lib/model.ts` beside it — rather than the absolute
//! path the host resolved, which would make a stream written on one machine
//! name every class differently from the same program on another, and would
//! make the bytes of a pickle depend on where the repository was checked out.

use rts_cranelift::ir::{FuncBuilder, ValueId};

use super::{Ctx, EmitResult};
use crate::runtime::RuntimeOp;

/// Registers one declaration, when this compilation registers any.
pub(super) fn declare(builder: &mut FuncBuilder, ctx: &mut Ctx, target: ValueId, name: &str) -> EmitResult<()> {
    let Some(module) = ctx.module_key.clone() else {
        return Ok(());
    };
    let module = ctx.literal(&module);
    let module = super::module::number(builder, u64::from(module));
    let name = ctx.literal(name);
    let name = super::module::number(builder, u64::from(name));
    let target = super::expr::tagged(builder, target);
    super::expr::call(builder, ctx, RuntimeOp::SerdeDeclare, &[target, module, name])?;
    Ok(())
}

/// A module's key: its path relative to the entry's directory, with `/`
/// between the parts on every platform, or `""` for the entry itself.
///
/// A module outside the entry's directory keeps the path the host gave it,
/// which is stable on one machine and not across two — the decoder's fallback
/// to the plain name, when it is unambiguous, is what covers that case.
pub(super) fn module_key(specifier: &str, entry: &str) -> String {
    if specifier == entry {
        return String::new();
    }
    let relative = std::path::Path::new(entry)
        .parent()
        .and_then(|base| std::path::Path::new(specifier).strip_prefix(base).ok());
    match relative {
        Some(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        None => specifier.replace('\\', "/"),
    }
}
