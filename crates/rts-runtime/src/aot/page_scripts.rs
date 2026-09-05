//! What `context.eval_compiler_with_receiver` answers in an AOT binary: a
//! page `<script>`'s pre-placed function, found by the hash of its source.
//!
//! # Why this exists
//!
//! `rts-dom-bridge`'s `DomScope.run` reaches
//! `rts_core::entry::evaluate_in_scope_with_receiver` for every `<script>` a
//! page runs, expecting a HOST to compile the text it hands over — the seam a
//! JIT run fills with `rts-host::live::evaluate_in_scope_with_receiver`,
//! which this binary does not carry (`rts-host` is a JIT host; this crate is
//! the facade an AOT program links, and installs no compiler at all, same as
//! it installs none for `eval` or `new Function`). Without this module the
//! seam stayed unfilled and every page `<script>` failed with "a fonte não
//! compilou" — true, but not why.
//!
//! `rts compile --html` closes it a different way: it compiles the page's own
//! `<script>` bodies at BUILD time (`rts_host::object::page`) into this same
//! binary, and writes `(source hash, function-table index)` pairs into the
//! manifest. This module is what turns that table, plus the addresses
//! `super::main` already resolved out of `FUNCTION_TABLE_SYMBOL`, into the
//! callback `rts_core::entry::declare_eval_compiler_with_receiver` wants.
//!
//! # Why a hash and not the text
//!
//! [`rts_core::entry::source_hash`]'s own header has the full reasoning; the
//! short form is that the two sides — `rts compile`'s process and this
//! binary's — never talk to each other, and a hash is one fixed-width
//! comparison per candidate instead of a string compare against a source that
//! may be a bundled framework, inlined.
//!
//! # What a miss means, and why it raises rather than answering `None` bare
//!
//! `evaluate_in_scope_with_receiver` returning `None` is exactly what an
//! ordinary JIT syntax error already does — `rts-dom-bridge`'s `scope.rs`
//! reports "a fonte não compilou" for it, which is a truthful but useless
//! answer here: the source is very likely well-formed, it simply was not
//! among the `--html` files this binary was built from. Raising a specific
//! `TypeError` first is what makes that distinction visible instead of
//! indistinguishable from a real syntax error — the same discipline
//! `rts-core`'s README rule 8 states for a native that finds it has nothing
//! to answer with: say why before giving up.
//!
//! This is also reached for `node:vm`'s `runInContext`/`runInThisContext` —
//! `live.rs`'s own header names both callers of the JIT seam this replaces —
//! and the message is worded to cover both rather than naming `--html` as if
//! it were the only door.

use std::sync::OnceLock;

use super::Entry;

/// `(source hash, resolved address)`, built once from the manifest's
/// `page_scripts` table and `FUNCTION_TABLE_SYMBOL`'s addresses.
static TABLE: OnceLock<Vec<(u64, Entry)>> = OnceLock::new();

/// Records what the manifest and the linker together said, once, before the
/// program runs.
///
/// `entries` pairs each page script's source hash with the address ALREADY
/// resolved for it — `super::main` reads that address out of
/// `FUNCTION_TABLE_SYMBOL` at the manifest's `page_scripts` index, which is
/// why this module needs no address table of its own.
pub fn declare(entries: Vec<(u64, Entry)>) {
    let _ = TABLE.set(entries);
}

/// The host callback `rts_core::entry::declare_eval_compiler_with_receiver`
/// installs: `fn(&str, u64, u64) -> Option<u64>`, source/environment/receiver
/// in, the completion value out.
///
/// `None` when no `--html` file this binary was compiled from carried this
/// exact source: the caller (`rts_core::entry::evaluate_in_scope_with_receiver`)
/// turns that into the ordinary "did not compile" report, AFTER this function
/// has already raised the more specific reason — see this module's own
/// header for why both happen.
pub fn evaluate_in_scope_with_receiver(source: &str, environment: u64, receiver: u64) -> Option<u64> {
    let hash = rts_core::entry::source_hash(source);
    let found = TABLE
        .get()
        .and_then(|table| table.iter().find(|(known, _)| *known == hash).map(|(_, entry)| *entry));
    let Some(entry) = found else {
        rts_core::entry::throw_type_error(
            "this script was not pre-compiled into this AOT binary — `rts compile --html` \
             only precompiles the <script> tags of the HTML files it was given, and \
             `node:vm`'s runInContext/runInNewContext have no compiler at all in an AOT \
             binary (rts-host README, \"what it does not do yet\")",
        );
        return None;
    };
    let nothing = rts_core::entry::undefined_value();
    // SAFETY: `entry` came out of `FUNCTION_TABLE_SYMBOL`, whose every address
    // is a function this same object placed under the convention `Entry`
    // spells — `super::main` already relies on that for `__rts_functions`'
    // other entries, and a page script is placed no differently.
    Some(unsafe { entry(environment, receiver, nothing, nothing, nothing, nothing) })
}
