//! The eight typed arrays, declared.
//!
//! # Why a `macro_rules!` under a proc macro
//!
//! The eight classes differ in three things — the name, the element kind and the
//! element width — and in nothing else. `#[rtse::class]` removes the duplication
//! *within* a class, between a member and its `extern "C"` wrapper; it cannot
//! remove the duplication *across* eight classes, because a proc macro sees one
//! item and cannot see its neighbours. So the member list is written once here
//! and instantiated eight times, and what a reader has to check is one list
//! rather than eight that must agree.
//!
//! The rejected alternative is eight written-out blocks. It is not merely longer:
//! it is where one of them ends up clamping differently, or answering a copy from
//! `subarray`, and the two would look equally plausible in a diff.
//!
//! # Why the generated names are passed in
//!
//! `register_int8_array` is derived by `#[rtse::class]` from the type ident, and
//! `macro_rules!` cannot concatenate identifiers — so each invocation states the
//! name the attribute will produce, and a mistake in one is a link error rather
//! than a wrong answer. The alternative, a `paste!`-style dependency, buys the
//! removal of one token per line.
//!
//! # What is not here
//!
//! `%TypedArray%`. The language gives all eight a shared prototype, so
//! `Object.getPrototypeOf(Int8Array.prototype)` is an object with every method on
//! it and each class's own prototype is nearly empty. Here each class carries its
//! own copy of the members, which costs cells and diverges for a program that
//! walks the chain looking for the shared one. Building it needs the attribute to
//! be able to name a prototype no class declares, which is a change to the
//! attribute rather than to this file.

use super::element::Kind;
use super::{Context, typed};
use crate::value::Value;

/// `BYTES_PER_ELEMENT` on the **constructor**, beside the one on the prototype.
///
/// # Why this is a wrapper and not a second `const` in the block
///
/// The language puts it on both — `Int8Array.BYTES_PER_ELEMENT` and
/// `new Int8Array(0).BYTES_PER_ELEMENT` both answer 1 — and `#[stat]` chooses one
/// of the two for a constant. Two constants with one Rust name in one `impl` is
/// not a thing Rust has, and `#[js]` does not apply to constants, so the second
/// spelling is installed here rather than by teaching the attribute a rule that
/// exists for exactly this class family.
fn per_element(context: &mut Context, made: u64, size: f64) {
    if let Some(cell) = Value(made).as_slot() {
        super::stamp(context, cell, "BYTES_PER_ELEMENT", size);
    }
}

/// One typed array class, over [`super::typed`].
macro_rules! declare {
    ($($ty:ident, $generated:ident, $wrapper:ident, $js:literal, $kind:expr, $size:literal;)+) => {
        $(
            #[rtse::class($js)]
            impl $ty {
                /// How wide one element is. Also on the constructor — see
                /// [`per_element`].
                const BYTES_PER_ELEMENT: f64 = $size;

                /// `new T(length)`, `new T(array)`, or
                /// `new T(buffer, byteOffset?, length?)`.
                #[construct]
                fn build(this: u64, source: u64, offset: u64, length: u64) -> u64 {
                    typed::construct(this, $kind, source, offset, length)
                }

                /// `t.at(i)` — a negative index counts from the end.
                fn at(this: u64, index: f64) -> u64 {
                    typed::element_at(this, index, true)
                }

                /// `t.get(i)` — not a method the language has. It is here
                /// because `t[i]` cannot reach the elements until
                /// `computed::get_indexed` learns about views, and a class
                /// nothing can read is a class nothing can test.
                fn get(this: u64, index: f64) -> u64 {
                    typed::element_at(this, index, false)
                }

                /// `t.setAt(i, v)` — the write half of the same gap. Named apart
                /// from `set`, which the language already gave to the bulk copy.
                fn set_at(this: u64, index: f64, value: f64) -> u64 {
                    typed::store_at(this, index, value)
                }

                /// `t.set(source, offset?)` — copies elements in, converting
                /// each through this array's element type.
                fn set(this: u64, source: u64, offset: u64) -> u64 {
                    typed::copy_from(this, source, offset)
                }

                /// `t.subarray(begin, end)` — another view over the same bytes.
                fn subarray(this: u64, begin: u64, end: u64) -> u64 {
                    typed::subarray(this, begin, end)
                }

                /// `t.slice(begin, end)` — a copy, in a buffer of its own.
                fn slice(this: u64, begin: u64, end: u64) -> u64 {
                    typed::slice(this, begin, end)
                }

                /// `t.fill(value, begin, end)` — the array, so calls chain.
                fn fill(this: u64, value: f64, begin: u64, end: u64) -> u64 {
                    typed::fill(this, value, begin, end)
                }
            }

            #[doc = concat!("Installs `", $js, "`, and its constructor's `BYTES_PER_ELEMENT`.")]
            pub(in crate::entry) fn $wrapper(context: &mut Context) -> u64 {
                let made = $generated(context);
                per_element(context, made, $size);
                made
            }
        )+

        /// The prototype instances of a kind inherit from, registering the class
        /// if nothing has read its name yet.
        ///
        /// Needed because `t.subarray()` answers a `Uint8Array` in a program that
        /// never wrote the word `Uint8Array` — the class has to exist before the
        /// global name is read, or the result would inherit nothing and
        /// `made.at(0)` would be `undefined`.
        pub(in crate::entry) fn ensure(context: &mut Context, kind: Kind) -> Option<u64> {
            $(
                if kind == $kind {
                    $wrapper(context);
                    return crate::entry::class_support::prototype(context, $js);
                }
            )+
            None
        }
    };
}

declare! {
    Int8Array, register_int8_array, int8_array, "Int8Array", Kind::Int8, 1.0;
    Uint8Array, register_uint8_array, uint8_array, "Uint8Array", Kind::Uint8, 1.0;
    Int16Array, register_int16_array, int16_array, "Int16Array", Kind::Int16, 2.0;
    Uint16Array, register_uint16_array, uint16_array, "Uint16Array", Kind::Uint16, 2.0;
    Int32Array, register_int32_array, int32_array, "Int32Array", Kind::Int32, 4.0;
    Uint32Array, register_uint32_array, uint32_array, "Uint32Array", Kind::Uint32, 4.0;
    Float32Array, register_float32_array, float32_array, "Float32Array", Kind::Float32, 4.0;
    Float64Array, register_float64_array, float64_array, "Float64Array", Kind::Float64, 8.0;
}
