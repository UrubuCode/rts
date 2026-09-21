//! A local whose every store is a bitwise operator is carried as an integer.
//!
//! Counted off the EMITTED IR rather than asked of the analysis, and that is the
//! whole point of the file. `rts-codegen`'s `emit/proven.rs` records three
//! separate occasions where a pass proved something the emitter did not spend —
//! the emitter widened at every store, the operand never arrived in the
//! representation the fast path looks for, and the fast path never ran. An
//! analysis-level test cannot see that; this one can, because a conversion that
//! survives is a line in the output.
//!
//! # Why `ToInt32` and not both directions
//!
//! Because only one of them is dead-code-free at this stage. A binding in the
//! integer representation still emits the `ToF64` of every READ — the widening
//! `binding::read` inserts so that no consumer had to learn a third case — and
//! where the use is a bitwise operator that value has no consumer and the code
//! generator drops it. Counting those would count instructions that never
//! execute, so the count would say "5 conversions" about a loop that performs
//! none. `ToInt32` has no such shadow: the machine's `fold::to_int32_answer`
//! settles it while building, so one in the output is one on the chain.
//!
//! What these programs MEAN is pinned elsewhere and against two other runtimes:
//! `tests/cross-runtime/numeric/437_int32_binding.ts`.

/// How many narrowings into the integer domain a compiled body performs.
fn narrowings(source: &str) -> usize {
    let ir = rts_host::describe::describe_source(source).expect("compiles");
    ir.lines().filter(|line| line.contains("ToInt32(")).count()
}

/// A loop whose accumulator is only ever assigned a bitwise result narrows
/// nothing, however many operators it runs.
///
/// The second program does twice the bitwise work of the first. Before
/// `emit/int32.rs` each store widened and each read narrowed again, so the count
/// grew with the operators; the binding is now in the integer representation
/// across the back edge and neither program converts at all.
#[test]
fn a_bitwise_accumulator_narrows_nothing() {
    let one = narrowings("function t(n){ let a = 0; for (let i = 0; i < n; i++) a = a ^ 3; return a; }");
    let two = narrowings(
        "function t(n){ let a = 0; for (let i = 0; i < n; i++) { a = a ^ 3; a = a & 255; } return a; }",
    );
    assert_eq!((one, two), (0, 0), "an integer binding stays one across the loop");
}

/// A binding an arithmetic store can reach keeps the double representation, so
/// the bitwise operator beside it still converts.
///
/// Not symmetry for its own sake. `a + 1` leaves the int32 range at
/// 2 147 483 647 — JavaScript answers 2 147 483 648 and this representation
/// cannot hold it — so a pass that claimed the binding anyway would answer a
/// wrapped negative number, and be fast and wrong.
#[test]
fn an_arithmetic_store_keeps_the_binding_a_double() {
    let mixed = narrowings(
        "function t(n){ let a = 0; for (let i = 0; i < n; i++) { a = a + 1; a = a ^ 3; } return a; }",
    );
    assert!(
        mixed > 0,
        "an accumulator `+` can reach must stay a double, so the `^` on it still \
         narrows — found {mixed}"
    );
}

/// `>>>` does not put a binding in the integer representation.
///
/// Its result is `ToUint32`: `-1 >>> 0` is 4 294 967 295, outside the range, and
/// the one place "bitwise" and "int32" come apart. The arm excluding it is a
/// single omission from a `matches!` and reads like an oversight, so it is
/// pinned against its own neighbour: the two programs differ by one character.
///
/// The `&` is what makes the test able to see anything. `>>>` alone is a runtime
/// call that narrows nothing either way, so a count over it would pass whether
/// the arm was right or wrong; beside a `&`, an admitted `>>>` would carry the
/// binding as an integer and the `&` would stop converting.
#[test]
fn an_unsigned_shift_leaves_the_binding_a_double() {
    let signed =
        narrowings("function t(n){ let a = 0; for (let i = 0; i < n; i++) { a = a >> 1; a = a & 255; } return a; }");
    let unsigned =
        narrowings("function t(n){ let a = 0; for (let i = 0; i < n; i++) { a = a >>> 1; a = a & 255; } return a; }");
    assert_eq!(signed, 0, "`>>` keeps the binding in the integer representation");
    assert!(
        unsigned > signed,
        "`>>>` must not preserve the integer representation — its native path \
         performs its own ToInt32 conversions and its result can reach 4 294 967 295; \
         found signed={signed}, unsigned={unsigned}"
    );
}
