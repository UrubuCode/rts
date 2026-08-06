//! `node:*` — the Node compatibility modules, for the new engine.
//!
//! # Why this is a crate and not part of `rts-std-rwk`
//!
//! Because the two answer different questions. `rts` is what THIS runtime
//! offers; `node:fs` is what a program written for another runtime expects to
//! find. They diverge — Node's `fs.readFileSync` answers a `Buffer` unless told
//! otherwise, and nothing about this engine wants that — and a crate holding
//! both would keep having to say which of the two a given decision belongs to.
//!
//! # Why it exists NOW and did not before
//!
//! Measured, twice. `node:*` is imported by 102 of the 818 files in the repo's
//! suite, and until modules compiled none of those files could be read at all —
//! so a module here would have been a structure with no producer, which is what
//! the crate READMEs call a gap rather than a feature.
//!
//! `rts-host-rwk`'s `suite_coverage` is what changed the answer: with modules
//! landed, 586 of the 818 compile, and the unbound names left in the ranking are
//! Node's. The plug is the same pair `rts-std-rwk` uses — `make_namespace` to
//! build one, `declare_module` to name it — so nothing new was invented here.
//!
//! # What every module in here owes the reader
//!
//! A named list of what it does NOT implement. A Node module is large and this
//! is a subset; a subset that pretends otherwise is how a program finds out at
//! run time, and this repository's rule is that a gap is refused by name rather
//! than approximated. `undefined` for a member that is absent is the honest
//! answer, and each module says which members those are.

#![deny(missing_docs)]
#![deny(dead_code)]

pub mod assert;
pub mod buffer;
pub mod events;
pub mod fs;
pub mod os;
pub mod path;
pub mod process;
pub mod querystring;
pub mod util;

use rts_core_rwk::entry::Context;

/// Registers every `node:` module this crate provides.
///
/// Under both spellings: `node:fs` and `fs`. The prefixed one is what a modern
/// program writes and the bare one is what the ecosystem is full of, and a
/// resolver that answered only one of them would refuse half the corpus for a
/// reason that has nothing to do with what it can do.
pub fn install(context: &mut Context) {
    // Built once and named twice. `fs::namespace` makes a NEW object every call,
    // so asking it a second time for `fs.promises` would register a specifier
    // pointing at an object no `fs` a program imported ever held.
    let files = fs::namespace(context);
    for (name, namespace) in [
        ("assert", assert::namespace(context)),
        ("buffer", buffer::namespace(context)),
        ("events", events::namespace(context)),
        ("fs", files),
        ("os", os::namespace(context)),
        ("path", path::namespace(context)),
        ("process", process::namespace(context)),
        ("querystring", querystring::namespace(context)),
        ("util", util::namespace(context)),
    ] {
        rts_core_rwk::entry::declare_module(context, &format!("node:{name}"), namespace);
        rts_core_rwk::entry::declare_module(context, name, namespace);
    }

    // `node:fs/promises` is its own specifier and resolves to the object `fs`
    // carries as `promises` — the SAME object, not a second one built beside it.
    // Two would be two answers to what `fs.promises.readFile === ` compares.
    let promises = rts_core_rwk::entry::get_member(context, files, "promises");
    rts_core_rwk::entry::declare_module(context, "node:fs/promises", promises);
    rts_core_rwk::entry::declare_module(context, "fs/promises", promises);
}
