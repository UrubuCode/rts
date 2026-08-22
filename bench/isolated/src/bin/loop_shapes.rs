//! **Experiment 2 — what the missing IR pass costs a loop.**
//!
//! # The question
//!
//! `rts ir` on `let a = 0; for (let i = 0; i < n; i++) a += arr[i & 1023];`
//! emits this, per iteration:
//!
//! ```text
//! block18:
//!     v63: I32 = ToInt32(v55)            i, which is F64
//!     v64: I32 = ToInt32(v62)            1023.0 — a CONSTANT, converted at run time
//!     v65: I32 = Bitwise(And, v63, v64)
//!     v66: F64 = ToF64(v65)
//!     v67: Tagged = Widen(v66)           re-boxed to be passed
//!     v68: Tagged = Call(__rts_get_indexed, [arr, v67])
//!     ...
//! block22:
//!     Guard { input: v54, expect: F64 }  v54 is `a`, arriving as a Tagged block parameter
//! block25:
//!     Guard { input: v68, expect: F64 }
//! block26:
//!     v76: F64 = FloatArith(Add, v74, v75)
//!     v77: Tagged = Widen(v76)           re-boxed for the back edge
//! ```
//!
//! Two of those are not the operation the program asked for:
//!
//! 1. **`ToInt32` over a constant, per iteration.** `crates/rts-cranelift/src/ir/fold.rs`
//!    folds exactly two things — a guard whose answer is known, and `x * 1.0` —
//!    and its own documentation says why nothing else: *"Anything needing a
//!    fixed point, a traversal, or knowledge of a second block belongs in a pass
//!    and not here."* There is no such pass, so the constant survives.
//!
//! 2. **The accumulator is `Tagged` across the loop's back edge**, so every
//!    iteration unboxes it (`Guard`) and re-boxes it (`Widen`). `fold.rs`
//!    declines this by name too: `guard_answer` answers about one instruction,
//!    and *"a value that reaches a block parameter through two widened
//!    predecessors answers `None`. That is a traversal, and a traversal is a
//!    pass."*
//!
//! **How much are those two worth?** `ToInt32` is not cheap —
//! `crates/rts-cranelift/src/lower/body.rs:280` lowers it to a seven-instruction
//! serial float chain (`trunc`, `fmul`, `trunc`, `fmul`, `fsub`,
//! `fcvt_to_sint_sat`, `ireduce`) and its own comment records `a = a | 0`
//! costing 11.6 ns on a dependency chain against 2.98 ns off it. So the answer
//! is plausibly large. Plausibly is not a number.
//!
//! # What is being compared
//!
//! Five shapes of the same loop, each adding one of the defects to the one
//! above it, so that every row's difference from its predecessor is the price of
//! exactly one missing transformation.
//!
//! The stand-in for `__rts_get_indexed` is an `#[inline(never)] extern "C"`
//! function that takes a machine `i64` index and returns an `f64`. It is
//! deliberately trivial: this experiment prices what the *caller* does around
//! the call, and giving the callee real work would bury the thing being
//! measured. What the callee itself costs is a different question and gets its
//! own experiment.
//!
//! # What this cannot say
//!
//! It cannot say the engine will move by this much. The engine's loop has other
//! register pressure and a real callee. What it can say — and this is the whole
//! point of running it first — is whether the transformations are worth writing
//! a pass for at all. A pass over the IR is weeks of work in a crate whose
//! README forbids reaching around the boundary to do it, so the number that
//! justifies starting has to exist before the work does.

use rts_isolated::{measure, opaque, report};

/// The NaN-boxing base, from `crates/rts-cranelift/src/tags/mod.rs:33`.
///
/// A word whose top bits match this is encoded; anything else is a genuine
/// double. So widening an `F64` is the identity on the bits, and guarding a
/// `Tagged` back to `F64` is "check the top bits do NOT match, then reinterpret"
/// — which is what the two shapes below model.
const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;

/// `Widen` over a value already proven `F64`: the bits, unchanged.
///
/// Free in isolation. It is not free in the loop, because it forces the value
/// through a general-purpose register and makes the block parameter `Tagged`,
/// which is what the guard on the other side then has to undo.
#[inline(always)]
fn widen_f64(value: f64) -> u64 {
    value.to_bits()
}

/// `Guard { expect: F64 }` over a `Tagged`: a test and a reinterpretation.
///
/// The failure path in the engine calls the runtime; here it is unreachable by
/// construction and marked so, which is the same shape the engine has (a cold
/// block) rather than a shortcut.
#[inline(always)]
fn guard_f64(word: u64) -> f64 {
    if word & BOX_BASE == BOX_BASE {
        // In the engine this branches to a block that calls the runtime. The
        // branch is what costs; where it goes does not, for this measurement.
        return opaque(0.0);
    }
    f64::from_bits(word)
}

/// `ToInt32`, exactly as `crates/rts-cranelift/src/lower/body.rs:280` lowers it.
///
/// Transcribed rather than replaced with `as i32`, because the whole question is
/// what *this chain* costs. Rust's `as i32` on `f64` is a saturating
/// `cvttsd2si` — one instruction — and would answer a different question.
#[inline(always)]
fn to_int32(x: f64) -> i32 {
    let truncated = x.trunc();
    let quotient = truncated * (1.0 / 4294967296.0);
    let whole = quotient.trunc();
    let carried = whole * 4294967296.0;
    let inside = truncated - carried;
    // `fcvt_to_sint_sat` then `ireduce`: saturate into i64, then take the low
    // 32 bits. Rust's `as` is saturating on float-to-int, which is the same.
    let wide = inside as i64;
    wide as i32
}

/// Stands in for `__rts_get_indexed`, taking a machine index.
#[inline(never)]
extern "C" fn element_machine(index: i64) -> f64 {
    opaque(index) as f64
}

/// Stands in for `__rts_get_indexed` as it is actually called: the index
/// arrives NaN-boxed, so the callee unboxes it before it can do anything.
#[inline(never)]
extern "C" fn element_tagged(index: u64) -> u64 {
    let as_double = guard_f64(index);
    widen_f64(opaque(as_double))
}

fn main() {
    // The mask, made opaque so that no shape gets it folded for free — the
    // engine does not fold it and the point is to price not folding it.
    //
    // Made opaque again INSIDE each loop that uses it, and that is not
    // belt-and-braces: `opaque` once out here leaves the conversion
    // loop-invariant, and LLVM hoists it. Hoisting is precisely the
    // transformation the engine does not perform, so a row that benefited from
    // it would be measuring a compiler this experiment is not about. The first
    // version of row 3 did exactly that and reported the constant conversion as
    // free.
    let mask_const: f64 = opaque(1023.0);

    let rows = vec![
        // ------------------------------------------------------------ 1
        measure("1. everything unboxed, machine index", |n| {
            let mut acc: f64 = 0.0;
            let mut i: f64 = 0.0;
            while i < n as f64 {
                let index = (i as i64) & 1023;
                acc += element_machine(index);
                i += 1.0;
            }
            acc as u64
        }),
        // ------------------------------------------------------------ 2
        measure("2. + ToInt32 on the index (not the mask)", |n| {
            let mut acc: f64 = 0.0;
            let mut i: f64 = 0.0;
            while i < n as f64 {
                let index = to_int32(i) & 1023;
                acc += element_machine(index as i64);
                i += 1.0;
            }
            acc as u64
        }),
        // ------------------------------------------------------------ 3
        measure("3. + ToInt32 on the CONSTANT mask too", |n| {
            let mut acc: f64 = 0.0;
            let mut i: f64 = 0.0;
            while i < n as f64 {
                let index = to_int32(i) & to_int32(opaque(mask_const));
                acc += element_machine(index as i64);
                i += 1.0;
            }
            acc as u64
        }),
        // ------------------------------------------------------------ 4
        measure("4. + the index passed NaN-boxed", |n| {
            let mut acc: f64 = 0.0;
            let mut i: f64 = 0.0;
            while i < n as f64 {
                let index = to_int32(i) & to_int32(opaque(mask_const));
                let boxed = widen_f64(index as f64);
                acc += guard_f64(element_tagged(boxed));
                i += 1.0;
            }
            acc as u64
        }),
        // ------------------------------------------------------------ 5
        measure("5. + accumulator Tagged across the back edge", |n| {
            // `acc` is a u64 here, exactly as the block parameter is Tagged in
            // the IR: unboxed at the top of the body, re-boxed at the bottom.
            let mut acc_boxed: u64 = widen_f64(0.0);
            let mut i: f64 = 0.0;
            while i < n as f64 {
                let index = to_int32(i) & to_int32(opaque(mask_const));
                let boxed = widen_f64(index as f64);
                let got = guard_f64(element_tagged(boxed));
                let acc = guard_f64(acc_boxed);
                acc_boxed = widen_f64(acc + got);
                i += 1.0;
            }
            guard_f64(acc_boxed) as u64
        }),
    ];

    report(
        "Experiment 2 - what the missing IR pass costs a loop",
        &rows,
    );
    println!();
    println!("Row 5 is the shape `rts ir` emits today. Each row above it removes");
    println!("exactly one transformation the engine does not perform, so the");
    println!("difference between two adjacent rows is that transformation's price.");
}
