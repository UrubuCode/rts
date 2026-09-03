//! What `require("./x")` and `import("./x")` name, in a binary with no loader.
//!
//! # Why an AOT binary cannot ask the same question a JIT run asks
//!
//! `rts_host::graph::resolve_specifier` joins the referrer's directory with the
//! specifier and then looks at the DISK — `./x`, then `./x.ts`, `./x.js`, and
//! the `index.*` inside a directory of that name. That is right for a host that
//! just read those files. It is not available here: an AOT binary is one file
//! plus a sidecar, may run on a machine that never had the sources, and must
//! not answer differently because a `x.js` happens to be next to it.
//!
//! # So the answers travel instead of the rule
//!
//! The loader resolved every relative specifier in the graph while compiling,
//! and `rts_host::object` writes those `(referrer, written, resolved)` triples
//! into the manifest. This looks one up. The RULE — which extension wins, what
//! a directory means, how a path is canonicalised — stays in the one place that
//! has always owned it, which is what stops this from becoming the second
//! resolver `rts-core`'s `dynamic_module` header warns about: `createRequire`
//! reproduced the loader's rule, said in its own comment that it had to match
//! "exactly", and stopped matching the day the loader started stripping
//! Windows's verbatim prefix.
//!
//! # What this therefore cannot do, stated rather than discovered
//!
//! A COMPUTED specifier. `require("./" + name)` is in no table, because there
//! was nothing in the tree to resolve while compiling — so it resolves in a JIT
//! run and is refused, by name, in an AOT one. That is the one place the two
//! destinations answer differently about a program, and it is a fact about the
//! destination: one of them has the files.

use std::sync::OnceLock;

/// The triples the manifest carried, in the order it carried them.
///
/// A process-global rather than something handed to the resolver, because
/// `rts_core::entry::Resolver` is a `fn` pointer and not a closure — for that
/// type's own reason: what a host would capture is the context. One AOT binary
/// runs one program, so a single `OnceLock` is the whole of the state.
static TABLE: OnceLock<Vec<(String, String, String)>> = OnceLock::new();

/// Records what the manifest said, once, before the program runs.
pub fn declare(resolutions: Vec<(String, String, String)>) {
    let _ = TABLE.set(resolutions);
}

/// What `(referrer, specifier)` names, from the table.
///
/// `None` for anything not in it — a bare name, a `node:` specifier, or a
/// computed one — which leaves the specifier as the program wrote it. That is
/// the same answer `rts_host::graph::resolve_specifier` gives for the first
/// two, and the divergence for the third is this module's own header.
pub fn resolve(from: &str, specifier: &str) -> Option<String> {
    let table = TABLE.get()?;
    table
        .iter()
        .find(|(referrer, written, _)| referrer == from && written == specifier)
        .map(|(_, _, resolved)| resolved.clone())
}
