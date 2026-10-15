//! `rts-corelib` — the native CoreLib layer, imported as `rts:core`.
//!
//! # What this is, on the .NET model
//!
//! .NET has `System.Private.CoreLib`: the one assembly the runtime knows
//! intrinsically, which reaches the native runtime through FCall/QCall and the
//! JIT's `[Intrinsic]` recognition, and on which the rest of the BCL (`System.*`)
//! is written in ordinary C#. Rust has the same split: `core` (the `#[lang]`
//! items and `core::intrinsics`) with `std` written on top of it.
//!
//! `rts:core` is that layer for this engine. It exposes the irreducible machine
//! primitives — the ones a program cannot write in the language because they
//! *are* the language's floor — so that everything above them can eventually be
//! ordinary TypeScript compiled through the same pipeline as user code.
//!
//! # Why the std still sits NATIVE on top of this, for now
//!
//! Moving a stdlib body from `.ts` to a native was measured at **124× at run
//! time** (skill `add-builtin-class`): today a `.ts` body crosses the runtime
//! door, is `Tagged`, and does not inline at the call-site. So `rts:core` is
//! built first and the std keeps its native bodies on top of it; the std→TS
//! migration is a later, measured step, gated on direct-call, int32 and the
//! inliner+escape-analysis landing. See `.docs-pendentes/`.
//!
//! # Mechanism
//!
//! An `rts:` module is a namespace object registered in the runtime's module
//! table. `import { x } from "rts:core"` lowers to `module_binding`, which looks
//! up `"rts:core"` and reads the property off it. So installing is exactly:
//! build a namespace of natives, then declare it under the specifier — the same
//! shape `rts-std` uses for `rts:test`, `rts:runtime`, `rts:json5`.

use rts_core::entry::{self, Context, Provided};

/// Registers the `rts:core` module.
///
/// Called from the boot sequence beside `rts_std::install`. Until it is wired
/// there, the crate compiles and the namespace is ready, but no running program
/// can resolve the specifier yet.
pub fn install(context: &mut Context) {
    let core = entry::make_namespace(context, CORE);
    entry::declare_module(context, "rts:core", core);
}

/// The primitives `rts:core` exports.
///
/// Deliberately minimal for now: one proven, side-effect-free pair that
/// establishes the import path end to end without committing an ABI the std is
/// not yet ready to consume. The real taxonomy (raw memory, slot access, the
/// int32 intrinsics, allocation hints) is populated once the first export set is
/// confirmed.
const CORE: &[(&str, Provided)] = &[
    ("f64ToBits", f64_to_bits),
    ("f64FromBits", f64_from_bits),
];

/// `f64ToBits(x)` — the IEEE-754 bit pattern of a number, as an integer.
///
/// A core primitive because the language has no way to spell it: it reinterprets
/// the bits rather than converting the value. Same 2^53 boundary caveat as
/// `rts`'s `num.f64_to_bits` — the tagged number crossing is a double.
extern "C" fn f64_to_bits(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let value = entry::number_of(a).unwrap_or(0.0);
    entry::make_number(value.to_bits() as f64)
}

/// `f64FromBits(bits)` — the inverse: a number from its IEEE-754 bit pattern.
extern "C" fn f64_from_bits(_e: u64, _this: u64, a: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let bits = entry::number_of(a).unwrap_or(0.0) as u64;
    entry::make_number(f64::from_bits(bits))
}
