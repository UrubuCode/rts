//! A string parameter crosses as a pointer and a length.
//!
//! # Why this test exists in this crate and not in a runtime one
//!
//! `rts-core-rwk` never takes a `&str`. Its strings live in the heap and travel
//! as slots, so nothing there exercises the trampoline. The surface that does is
//! `rts-std` and `rts-node`, where a measurement over the existing boundary
//! found 119 `&str` parameters in `rts-node` alone — the second most common type
//! after the tagged value.
//!
//! So the trampoline was written for callers that do not use this macro yet, and
//! it is tested here rather than left to be discovered by the first one.
//!
//! Spelled `rts_macro_rwk::entry` rather than `rtse::entry`: a crate cannot
//! rename a dependency on itself, and consumers get the short spelling from
//! their own manifest.

use rts_cranelift::abi::{AbiType, Convention};
use rts_cranelift::repr::Repr;

/// A function whose parameters are all scalars is rewritten in place: no
/// trampoline, no call, nothing added.
#[rts_macro_rwk::entry]
pub fn doubled(value: i64) -> i64 {
    value * 2
}

/// A function taking a string keeps its ordinary Rust signature, and gains an
/// `extern "C"` neighbour that takes the pointer and the length.
#[rts_macro_rwk::entry]
pub fn text_length(text: &str) -> i64 {
    text.len() as i64
}

/// Mixed, to prove the rewriting is per-parameter rather than per-function.
#[rts_macro_rwk::entry]
pub fn nth_byte(text: &str, index: i64) -> i64 {
    text.as_bytes().get(index as usize).map_or(-1, |b| *b as i64)
}

#[test]
fn a_string_parameter_is_one_slice_not_two_slots() {
    assert_eq!(
        TEXT_LENGTH_ENTRY.params,
        &[AbiType::Slice(Repr::I8)],
        "one logical argument, which is the improvement on the interface this \
         replaced — there a string was two loose slots a caller had to remember \
         to pass together"
    );
    assert_eq!(TEXT_LENGTH_ENTRY.convention, Convention::Foreign);
}

#[test]
fn mixing_a_string_with_a_scalar_keeps_both_shapes() {
    assert_eq!(
        NTH_BYTE_ENTRY.params,
        &[AbiType::Slice(Repr::I8), AbiType::Scalar(Repr::I64)],
    );
}

#[test]
fn a_scalar_only_function_pays_nothing() {
    assert_eq!(
        DOUBLED_ENTRY.params,
        &[AbiType::Scalar(Repr::I64)],
    );
}

#[test]
fn the_trampoline_reaches_the_rust_function() {
    // The point of the test: call through the exported ABI shape, not through
    // the Rust function, because the Rust function was never in doubt.
    unsafe extern "C" {
        #[link_name = "__rts_text_length"]
        fn raw_length(ptr: *const u8, len: usize) -> i64;
        #[link_name = "__rts_nth_byte"]
        fn raw_nth(ptr: *const u8, len: usize, index: i64) -> i64;
    }

    let text = "hello";
    assert_eq!(unsafe { raw_length(text.as_ptr(), text.len()) }, 5);
    assert_eq!(unsafe { raw_nth(text.as_ptr(), text.len(), 1) }, b'e' as i64);
    assert_eq!(
        unsafe { raw_nth(text.as_ptr(), text.len(), 99) },
        -1,
        "past the end is the function's own answer, not the trampoline's"
    );
}

#[test]
fn an_empty_string_survives_the_crossing() {
    unsafe extern "C" {
        #[link_name = "__rts_text_length"]
        fn raw_length(ptr: *const u8, len: usize) -> i64;
    }

    // A dangling-but-aligned pointer with length zero is what an empty slice
    // is in Rust, so the trampoline must not treat it as absent.
    let empty = "";
    assert_eq!(unsafe { raw_length(empty.as_ptr(), 0) }, 0);
}

// ---------------------------------------------------------------------------
// The spelling that costs nothing.
//
// A JavaScript string is a sequence of UTF-16 code units, so `&[u16]` is the
// shape a runtime string already has. These tests exist to pin the two things
// that follow from that and would otherwise be assumed: the descriptor says
// `I16` rather than `I8`, and a lone surrogate survives.

/// Counts code units, which is what `String.prototype.length` counts.
#[rts_macro_rwk::entry]
pub fn unit_count(units: &[u16]) -> i64 {
    units.len() as i64
}

/// Returns one code unit, which is what `charCodeAt` returns.
#[rts_macro_rwk::entry]
pub fn unit_at(units: &[u16], index: i64) -> i64 {
    units.get(index as usize).map_or(-1, |u| *u as i64)
}

#[test]
fn code_units_and_bytes_are_not_the_same_descriptor() {
    assert_eq!(
        UNIT_COUNT_ENTRY.params,
        &[AbiType::Slice(Repr::I16)],
        "the element is the whole point: before it existed, this and \
         `text_length` had identical descriptors while taking genuinely \
         different memory"
    );
    assert_ne!(
        UNIT_COUNT_ENTRY.params, TEXT_LENGTH_ENTRY.params,
        "UTF-16 code units and UTF-8 bytes must be distinguishable at the \
         boundary, or nothing can tell a caller which one re-encodes"
    );
}

#[test]
fn a_lone_surrogate_crosses_intact() {
    unsafe extern "C" {
        #[link_name = "__rts_unit_count"]
        fn raw_count(ptr: *const u16, len: usize) -> i64;
        #[link_name = "__rts_unit_at"]
        fn raw_at(ptr: *const u16, len: usize, index: i64) -> i64;
    }

    // `"\u{1F600}"[0]` in JavaScript. Legal, and not valid Unicode — a UTF-8
    // crossing would have to abort on it, which is why `&[u16]` exists.
    let units: [u16; 2] = [0xD83D, 0x0041];
    assert_eq!(unsafe { raw_count(units.as_ptr(), units.len()) }, 2);
    assert_eq!(unsafe { raw_at(units.as_ptr(), units.len(), 0) }, 0xD83D);
}

#[test]
fn the_length_is_in_elements_not_bytes() {
    unsafe extern "C" {
        #[link_name = "__rts_unit_count"]
        fn raw_count(ptr: *const u16, len: usize) -> i64;
    }

    // Pinned because it is the mistake this crossing invites: four code units
    // occupy eight bytes, and a trampoline that forwarded the byte count would
    // read past the end and still usually return something.
    let units: [u16; 4] = [b'a' as u16, b'b' as u16, b'c' as u16, b'd' as u16];
    assert_eq!(unsafe { raw_count(units.as_ptr(), units.len()) }, 4);
}
