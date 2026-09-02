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
use super::{Context, typed, typed_order, typed_species, typed_visit};
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
            #[rtse::class($js, tag)]
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
                ///
                /// The value crosses as it arrived, not as a number: a bigint
                /// element takes a bigint, and coercing at the wrapper would
                /// have left the two 64-bit classes unwritable through this
                /// spelling while `t[i] = v` worked.
                fn set_at(this: u64, index: f64, value: u64) -> u64 {
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

                /// `t.slice(begin, end)` — a copy, in a buffer of its own,
                /// and in the class the species protocol names.
                fn slice(this: u64, begin: u64, end: u64) -> u64 {
                    typed_species::slice(this, begin, end)
                }

                /// `t.copyWithin(target, start, end)` — elements moved
                /// within this same view, answering it so calls chain.
                fn copy_within(this: u64, target: u64, start: u64, end: u64) -> u64 {
                    typed_order::copy_within(this, target, start, end)
                }

                /// `t.sort(compare)` — in place, and NUMERICALLY when no
                /// comparator is given, which is where this differs from an
                /// array's.
                fn sort(this: u64, compare: u64) -> u64 {
                    typed_order::sort(this, compare)
                }

                /// `t.fill(value, begin, end)` — the array, so calls chain.
                fn fill(this: u64, value: u64, begin: u64, end: u64) -> u64 {
                    typed::fill(this, value, begin, end)
                }

                /// `t.includes(search, from)`.
                fn includes(this: u64, search: u64, from: u64) -> bool {
                    typed::includes(this, search, from)
                }

                /// `t.indexOf(search, from)` — strict equality, so it and
                /// `includes` disagree about `NaN`. See [`typed::index_of`].
                #[js("indexOf")]
                fn index_of(this: u64, search: u64, from: u64) -> f64 {
                    typed::index_of(this, search, from)
                }

                /// `t.lastIndexOf(search, from)`.
                #[js("lastIndexOf")]
                fn last_index_of(this: u64, search: u64, from: u64) -> f64 {
                    typed::last_index_of(this, search, from)
                }

                /// `t.join(separator)`.
                ///
                /// Present because its absence was not a missing feature but a
                /// missing NAME: `t.join(",")` is a `TypeError` on a method the
                /// language defines, and `String(t)` reaches it through
                /// `Array.prototype.toString`, so a typed array printed as
                /// `[object Uint8Array]` where every other engine prints its
                /// elements.
                fn join(this: u64, separator: u64) -> u64 {
                    typed::join(this, separator)
                }

                /// `t.values()`. `for`-`of` and spread reach a typed array's
                /// elements directly — `iterate::iterate` knows a view — so
                /// no `[Symbol.iterator]` member is installed here; only this
                /// named method, which the elder engine's tests reach through
                /// `[...t.values()]`.
                fn values(this: u64) -> u64 {
                    typed::values(this)
                }

                /// `t[Symbol.iterator]()` — the same iterator `values()` gives.
                ///
                /// # Why this is here after all
                ///
                /// [`typed::values`]'s own note says no `Symbol.iterator` member
                /// is installed, because `for`-`of` and spread reach a view's
                /// elements directly. That was true and was not enough:
                /// `const [a, b] = t` does NOT go through either. Array
                /// destructuring reads `Symbol.iterator` off the source, and
                /// when it is absent falls back to `Iterator.from(t)`, which
                /// treats the value ITSELF as the iterator — so `next` was
                /// `undefined` and a destructuring that every other engine runs
                /// ended the program.
                ///
                /// It is a member rather than a special case in that fallback
                /// because a typed array genuinely HAS this method in the
                /// language, and a program can read it: teaching one caller
                /// about views would have left `t[Symbol.iterator]` answering
                /// `undefined` to everybody else.
                #[js("@@iterator")]
                fn iterator(this: u64) -> u64 {
                    typed::values(this)
                }

                /// `t.keys()`.
                fn keys(this: u64) -> u64 {
                    typed::keys(this)
                }

                /// `t.entries()`.
                fn entries(this: u64) -> u64 {
                    typed::entries(this)
                }

                /// `T.from(source, mapFn, thisArg)`.
                ///
                /// A STATIC, and the only one here — every other member is an
                /// instance's. Its absence read as a broken engine rather than
                /// as a gap: `Uint8Array.from([1,2])` is how most programs
                /// build one at all.
                #[stat]
                fn from(source: u64, mapper: u64, receiver: u64) -> u64 {
                    typed_visit::from($kind, source, mapper, receiver)
                }

                /// `T.of(...items)` — a typed array of exactly the arguments,
                /// where [`from`](Self::from) takes one iterable.
                ///
                /// The second static, and it goes through `from` rather than
                /// beside it: `of` is `from` over an array of the arguments, and
                /// writing the element conversion twice is how the two end up
                /// clamping differently — which is the failure this whole file
                /// exists to prevent (see its module doc).
                ///
                /// `rest_arguments` is called OUTSIDE any borrow, because it is
                /// an entry point that takes the ambient one; a nested borrow
                /// aborts the process rather than failing.
                ///
                /// # The divergence, stated
                ///
                /// `rest_arguments` drops trailing `undefined`, which is right
                /// for a rest parameter and wrong here by one case:
                /// `Uint8Array.of(1, undefined)` answers a length of 1 where
                /// Node answers 2 (`undefined` becomes `NaN` becomes 0). A
                /// native has four argument slots and no count beside them, so
                /// telling "two arguments, the second `undefined`" from "one
                /// argument" needs something this boundary does not carry.
                /// Passing an explicit `undefined` to `of` is the whole of what
                /// is lost, and answering a length that is right for every other
                /// call was judged better than refusing all of them.
                #[stat]
                fn of(a0: u64, a1: u64, a2: u64, a3: u64) -> u64 {
                    let items = crate::entry::functions::rest_arguments(0, a0, a1, a2, a3);
                    let absent = crate::entry::undefined_value();
                    typed_visit::from($kind, items, absent, absent)
                }

                /// `t.forEach(callback, thisArg)`.
                fn for_each(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::for_each(this, callback, receiver)
                }

                /// `t.map(callback, thisArg)` — a new array of THIS kind, which
                /// is where it differs from an array's: the answers are written
                /// back through this class's element conversion.
                fn map(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::map(this, callback, receiver)
                }

                /// `t.filter(callback, thisArg)`.
                fn filter(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::filter(this, callback, receiver)
                }

                /// `t.find(callback, thisArg)`.
                fn find(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::find(this, callback, receiver)
                }

                /// `t.findIndex(callback, thisArg)`.
                fn find_index(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::find_index(this, callback, receiver)
                }

                /// `t.findLast(callback, thisArg)`.
                fn find_last(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::find_last(this, callback, receiver)
                }

                /// `t.findLastIndex(callback, thisArg)`.
                fn find_last_index(this: u64, callback: u64, receiver: u64) -> u64 {
                    typed_visit::find_last_index(this, callback, receiver)
                }

                /// `t.some(callback, thisArg)`.
                fn some(this: u64, callback: u64, receiver: u64) -> bool {
                    typed_visit::some(this, callback, receiver)
                }

                /// `t.every(callback, thisArg)`.
                fn every(this: u64, callback: u64, receiver: u64) -> bool {
                    typed_visit::every(this, callback, receiver)
                }

                /// `t.reduce(callback, initial)`.
                fn reduce(this: u64, callback: u64, initial: u64) -> u64 {
                    typed_visit::reduce(this, callback, initial)
                }

                /// `t.reduceRight(callback, initial)`.
                fn reduce_right(this: u64, callback: u64, initial: u64) -> u64 {
                    typed_visit::reduce_right(this, callback, initial)
                }

                /// `t.reverse()` — in place, answering the receiver.
                fn reverse(this: u64) -> u64 {
                    typed_visit::reverse(this)
                }

                /// `t.toReversed()` — a copy.
                fn to_reversed(this: u64) -> u64 {
                    typed_visit::to_reversed(this)
                }

                /// `t.toSorted(compare)` — a copy, over the same sort.
                fn to_sorted(this: u64, compare: u64) -> u64 {
                    typed_visit::to_sorted(this, compare)
                }

                /// `t.with(index, value)` — a copy with one element replaced,
                /// and a `RangeError` for an index the array does not have,
                /// which is what separates it from `t[i] = v`.
                fn with(this: u64, index: f64, value: u64) -> u64 {
                    typed_visit::with(this, index, value)
                }

                /// `t.toString()`.
                fn to_string(this: u64) -> u64 {
                    typed_visit::to_string(this)
                }
            }

            #[doc = concat!("Installs `", $js, "`, and its constructor's `BYTES_PER_ELEMENT`.")]
            pub(in crate::entry) fn $wrapper(context: &mut Context) -> u64 {
                let made = $generated(context);
                per_element(context, made, $size);
                made
            }
        )+

        /// The class name a kind's instances answer to.
        ///
        /// `None` for `Kind::Raw`, which is a `DataView` and not one of these —
        /// the one kind that is a view without being a typed array, and the
        /// reason this answers an `Option` rather than a name it would have to
        /// invent.
        ///
        /// Written off the same list as everything else here, because a second
        /// table from kind to name is the one that would come to disagree about
        /// `Uint8ClampedArray`.
        pub(in crate::entry) fn named(kind: Kind) -> Option<&'static str> {
            $(
                if kind == $kind {
                    return Some($js);
                }
            )+
            None
        }

        /// The prototype instances of a kind inherit from, registering the class
        /// if nothing has read its name yet.
        ///
        /// Needed because `t.subarray()` answers a `Uint8Array` in a program that
        /// never wrote the word `Uint8Array` — the class has to exist before the
        /// global name is read, or the result would inherit nothing and
        /// `made.at(0)` would be `undefined`.
        pub(in crate::entry) fn ensure(context: &mut Context, kind: Kind) -> Option<u64> {
            // ASKED first, registered only if the answer is no. The table is a
            // `Vec` scanned by comparing class NAMES, and `t.subarray()` paid
            // two of those scans — one inside the registration's own idempotence
            // check, one to read the prototype back — for a class that is
            // already there on every call but the first.
            $(
                if kind == $kind {
                    if let Some(found) = crate::entry::class_support::prototype(context, $js) {
                        return Some(found);
                    }
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
    Uint8ClampedArray, register_uint8_clamped_array, uint8_clamped_array,
        "Uint8ClampedArray", Kind::Uint8Clamped, 1.0;
    Int16Array, register_int16_array, int16_array, "Int16Array", Kind::Int16, 2.0;
    Uint16Array, register_uint16_array, uint16_array, "Uint16Array", Kind::Uint16, 2.0;
    Int32Array, register_int32_array, int32_array, "Int32Array", Kind::Int32, 4.0;
    Uint32Array, register_uint32_array, uint32_array, "Uint32Array", Kind::Uint32, 4.0;
    Float32Array, register_float32_array, float32_array, "Float32Array", Kind::Float32, 4.0;
    Float64Array, register_float64_array, float64_array, "Float64Array", Kind::Float64, 8.0;
    // The two whose elements are BIGINTS rather than numbers. They are one line
    // each here for the same reason the other eight are: the width and the
    // conversion live in `element`, and a class declaration says only which.
    BigInt64Array, register_big_int64_array, big_int64_array,
        "BigInt64Array", Kind::BigInt64, 8.0;
    BigUint64Array, register_big_uint64_array, big_uint64_array,
        "BigUint64Array", Kind::BigUint64, 8.0;
}
