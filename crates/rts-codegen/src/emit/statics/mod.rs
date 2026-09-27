//! `Number.isNaN`, `Array.isArray`, `Object.is` and the two global predicates,
//! decided in this crate.
//!
//! The rule `emit/math` states, applied to the next names a program reaches
//! without meaning an object: **anything low-level a program reaches by a
//! well-known name is decided in this crate, not left to the runtime as a
//! call.** Each of these cost 38 to 45 ns as a global read, a property read
//! through the chain cache and a native dispatch, for a body that is one or two
//! comparisons (`docs/codegen/native-call-floor.md`). Under the proof that the
//! name is still the language's, they are:
//!
//! - **instructions** over a proven double — `Number.isNaN`, `isFinite`,
//!   `isInteger`, `isSafeInteger`, and the global `isNaN`/`isFinite`, whose
//!   `ToNumber` a proven double has already paid. Stated once in
//!   `sequence.rs` for both emitters;
//! - **a direct call**, argument and answer unboxed — `Array.isArray`, which
//!   asks a side table, and `Object.is`, which is `SameValue` over two words.
//!
//! # The proof, and why it is the stricter one
//!
//! `primordial::only_a_base` for the three objects: the name must never leave
//! the position `Number.member`, because a program that copies `Object` into a
//! variable or passes it somewhere may have patched it out of sight. `Math`
//! rests on the looser `untouched` and a patched `Math.sin` is a thing nobody
//! does; a patched `Array.isArray` is a polyfill. The two global functions use
//! `untouched`, since a bare function name has no members to patch.

pub(crate) mod body;
pub(crate) mod sequence;

pub(super) use body::emit;
pub(crate) use body::Primordials;
