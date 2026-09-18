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
//! what bounds the registry by the source text. And every named function
//! declared at the TOP LEVEL of a module or script — Python's line for pickling
//! a function by reference: a closure's captured state has no name, and an
//! arrow has no name of its own. `hoist` emits that one, beside the closure it
//! registers, and says why it is not a pass of its own.
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
///
/// `space` is the number the class's own `#private` names were interned under
/// ([`private_space`]), so the runtime can write a private field by its depth
/// in the class chain rather than by a number that depends on where in the
/// source the class was written. `None` for a function, and for a class
/// declaring no private member.
pub(super) fn declare(
    builder: &mut FuncBuilder,
    ctx: &mut Ctx,
    target: ValueId,
    name: &str,
    space: Option<u32>,
) -> EmitResult<()> {
    let Some(module) = ctx.module_key.clone() else {
        return Ok(());
    };
    let module = ctx.literal(&module);
    let module = super::module::number(builder, u64::from(module));
    let name = ctx.literal(name);
    let name = super::module::number(builder, u64::from(name));
    // `u64::MAX` is the I64 `-1`: no space.
    let space = super::module::number(builder, space.map_or(u64::MAX, u64::from));
    let target = super::expr::tagged(builder, target);
    super::expr::call(builder, ctx, RuntimeOp::SerdeDeclare, &[target, module, name, space])?;
    Ok(())
}

/// The number a class's own private names carry — `@@#<n>#name`, the spelling
/// `parse::Cx::private_name` interns — read off the first private member the
/// body declares, or `None` when it declares none.
///
/// Read from the tree rather than threaded down from the parser because every
/// private member of one body carries the same number, and the tree already
/// holds it in the names it interned.
pub(super) fn private_space(ctx: &Ctx, class: &crate::syntax::Class) -> Option<u32> {
    class.body.iter().find_map(|element| match element.key()? {
        crate::syntax::ClassKey::Private(name) => {
            let text = ctx.names.text(*name).strip_prefix("@@#")?;
            text.split_once('#')?.0.parse().ok()
        }
        _ => None,
    })
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
