//! One number for a page `<script>`'s exact source, agreed by two processes
//! that never talk to each other.
//!
//! # Why this exists
//!
//! `rts compile --html` precompiles a page's `<script>` bodies at BUILD time,
//! into functions an AOT binary carries beside its manifest. At RUN time the
//! same source text arrives again — `rts-dom-bridge`'s `DomScope.run`, unaware
//! anything was precompiled, hands `rts_core::entry::evaluate_in_scope_with_receiver`
//! the exact bytes `crates/rts-dom/src/dom.ts`'s `__runScriptAt` extracted —
//! and the AOT facade's installed `eval_compiler_with_receiver` has to find
//! the RIGHT precompiled function among however many the page has. A hash of
//! the source is the lookup key; this is the one function both sides call, so
//! a build that hashed one way and a binary that hashed another cannot drift
//! into being unable to find each other.
//!
//! # Why a hash and not the text itself
//!
//! The text lives in `rts-runtime`'s manifest today only as what the AOT
//! binary was SEEDED with — property keys, string literals — never as a table
//! of whole program sources to compare byte-for-byte at every `<script>`. A
//! fixed-width number is one comparison per candidate instead of a string
//! compare against a source that may be hundreds of kilobytes (a bundled
//! framework, inlined).
//!
//! # Why this is not a cryptographic hash
//!
//! Nothing here defends against an adversary choosing a second script to
//! collide with the first — the input is the program's OWN page, decided by
//! whoever ran `rts compile --html`, not attacker-controlled text arriving
//! over a boundary. That is the same argument `rts-dom`'s `fasthash.rs` makes
//! for its own non-cryptographic hasher, and it is why this crate does not
//! reach for one either — the FNV-1a below is a fixed, public algorithm this
//! module owns outright rather than a dependency this crate would need to
//! justify carrying onto wasm.
//!
//! # Why FNV-1a specifically
//!
//! It has no seed to disagree about. `std::collections::hash_map::DefaultHasher`
//! is SipHash with a per-process random key — the two SIDES of this lookup are
//! two different PROCESSES (`rts compile`'s and the AOT binary's), so a random
//! seed would make every lookup miss. FNV-1a is one multiplication and one XOR
//! per byte, entirely specified by its constants, and is exactly as
//! deterministic across processes, platforms and Rust versions as this
//! contract needs.
//!
//! # 64 bits, and what that costs
//!
//! A page's own `<script>` count is a handful, not a corpus — the collision
//! risk this width carries is the one a `HashMap` already carries for every
//! key in this engine's shape trees, and is not the thing to spend a wider,
//! slower hash on. A collision here answers a script's own hash to a
//! DIFFERENT script than the one that wrote it, running the wrong compiled
//! function; nothing before this contract prevents it, so a widened hash
//! would be a real request from a real corpus in front of it — the shape
//! `perf-claim` asks any change like this to have.

/// FNV-1a's offset basis, 64-bit.
const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a's prime, 64-bit.
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// The hash `rts compile --html` writes into a program's manifest and an AOT
/// binary's `eval_compiler_with_receiver` hook recomputes to look a `<script>`
/// up by.
///
/// Over the UTF-8 bytes of `source`, exactly as both sides hold it: the
/// EXTRACTED text a `<script>` node carries — after a `data:` URI is decoded,
/// before anything wraps or lowers it — never the HTML around it and never a
/// compiled form of it. Two callers computing this over differently-trimmed
/// text would agree on nothing; see this module's own header for where each
/// side calls it from.
pub fn source_hash(source: &str) -> u64 {
    let mut hash = OFFSET_BASIS;
    for byte in source.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::source_hash;

    /// The behaviour this whole module exists to pin: the same text hashes
    /// the same way every time, which is the entire contract two separate
    /// processes rely on to find each other.
    #[test]
    fn the_same_source_hashes_the_same_way() {
        let source = "document.getElementById(\"x\").textContent = \"ok\";";
        assert_eq!(source_hash(source), source_hash(source));
    }

    /// A hash answers something ELSE for text that differs at all — not a
    /// correctness requirement (collisions are allowed, see the module
    /// header), but a sanity check that this is not a constant in disguise.
    #[test]
    fn different_source_usually_hashes_differently() {
        assert_ne!(source_hash("a"), source_hash("b"));
        assert_ne!(source_hash(""), source_hash("a"));
    }

    /// Byte-exact: a single character changes the answer. A lookup keyed on
    /// this hash is only as faithful as this property — a script trimmed by
    /// one space at build time and not at run time (or the reverse) must miss
    /// rather than silently run the wrong function.
    #[test]
    fn a_single_byte_changes_the_hash() {
        assert_ne!(source_hash("let x = 1;"), source_hash("let x = 1; "));
    }
}
