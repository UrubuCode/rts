//! A string's `Str` inside its own region cell, instead of in a slab beside it.
//!
//! # The measurement this exists for
//!
//! A string cost two allocations and two cold cache lines: a region cell
//! carrying the identity, and a `Str` in `Context::cells` carrying the text.
//! Decomposed on 2026-10-01, release, this machine:
//!
//! | | ns |
//! |---|---:|
//! | `region.alloc` nua | 8.00 |
//! | a cell with two slots written | **9.94** |
//! | `cells.insert` on its own | 10.14 |
//! | the same cell with the insert between the writes | **58.77** |
//! | `make_string(5)` | 48.50 |
//! | `intern_value(5)` | 80.37 |
//! | C#, `new char[5]` | 5.9 |
//!
//! The 49 nanoseconds between rows two and four are **not** the insert, which
//! costs ten on its own. They are the second cache line: the cell is in the
//! region and the `Str` is in the slab, and touching both is two misses where
//! one would do. That is what this removes, and the bytes were never the cost
//! — `Str::from_latin1(5)` measures 0.00, because `Narrow::Short` already
//! holds up to thirty of them inline.
//!
//! # Why this became possible on 2026-10-01 and not before
//!
//! The payload was outside the cell because the tracer walked every word of
//! every cell and would have followed a Rust `Vec`'s pointer as a reference.
//! `gc::traces_field` ended that: a slot the cell's layout declares non-GC is
//! skipped, so declared bytes are admissible where undeclared ones were not.
//! `docs/engine/one-form-per-question.md` records the reason as expired and
//! `principles.md` P3 names this work as the other half of deriving "which
//! words are references" from the type, the way Go and C# do.
//!
//! # Why only the strings that own nothing
//!
//! A cell goes back to the free list; nothing runs a destructor on it, and
//! nothing walks the region on teardown to run one either. So an owning `Str`
//! placed here would leak its buffer. A `drop_in_place` in the sweep was
//! refused because it fixes the half that is visible — a collected string —
//! and leaves the region's own teardown leaking, which is the half nothing
//! would notice. [`Str::owns_nothing`] makes the question unaskable instead:
//! an inline Latin-1 run lives in its cell, and a spilled run or any UTF-16
//! string keeps the slab.
//!
//! That is not a narrow case. `Narrow::Short` holds thirty bytes, which is
//! most of what a program's strings are — a property name, a key out of
//! `JSON.parse`, a regex group, a `split` piece.
//!
//! # Why the memo on `Str` survives the move
//!
//! `Str::key`'s own documentation argues the key memo belongs on the `Str`
//! rather than in a cell slot, because "a payload slot already has two owners
//! — the collector walks it as a possible reference, and a shape assigns
//! properties to slots". Both owners are answered here rather than ignored,
//! and that is why the argument does not carry over:
//!
//! - **The collector.** These slots are declared `Repr::I64`, so
//!   `gc::traces_field` answers false and `trace::edges_of` skips them. That
//!   declaration is the thing that did not exist when the memo was placed.
//! - **The shape.** `Context::shape_of` answers `None` for a string, so no
//!   shape assigns a property to any slot of a string cell. There was never a
//!   second owner here; the sentence was about object cells.
//!
//! The memo is still on the `Str`, which is still allocated and freed with the
//! string it belongs to. Only where that `Str` sits changed.
//!
//! # The open question, stated rather than buried
//!
//! [`at`] produces its `&Str` out of a `&[u64]` window, and the memo writes
//! through it are writes through a **shared** provenance into memory that is
//! not declared as holding an `UnsafeCell`. That is formally undefined
//! behaviour and Miri would flag it. It is not a bug today — only the memo
//! words are written, only through `Str`'s own accessors, and `Region::words`
//! is a `Vec` behind a `&Region`, so nothing is told the buffer is immutable —
//! but "no compiler has exploited it yet" is not the same as sound, and
//! writing that down is cheaper than rediscovering it.
//!
//! Three ways out, with the one this will take:
//!
//! - **`Region::words: Vec<UnsafeCell<u64>>`.** The real fix, and the argument
//!   for it is not this module: the region is ALREADY written behind shared
//!   references, because `Region::base` hands out an address and compiled code
//!   stores into cells through it while the runtime may hold a `&Region`. So
//!   the current type is a false statement about memory that is already
//!   shared-mutable, and this is merely the first caller to depend on it.
//!   Twenty-seven sites in four files of `heap/region/`.
//! - **The memo in a cell SLOT instead**, written through `&mut self.region`
//!   after the read borrow ends. Sound, and no `UnsafeCell` anywhere — but it
//!   leaves two memo mechanisms for one question, one for a slab string and
//!   one for a cell string, which is the shape the 2026-10-01 audit exists to
//!   refuse.
//! - **No memo for an in-cell string.** Refused: `Str::key`'s own measurement
//!   is that resolving without it costs three nanoseconds per character of a
//!   property name.
//!
//! The first, and separately, because it changes the hottest type in the
//! runtime and P8 says a change is measured before it lands — including one
//! justified by correctness.

use crate::text::Str;

use super::Context;

/// What slot 0 holds when the text is in this cell rather than in the slab.
///
/// # Why a sentinel and not a second type
///
/// A string is recognised by its header type, and about ten sites compare
/// against `Context::text_type` — `is_text_at`, `prototype_of`, `shape_of`,
/// the sweep, and the inline caches compiled code resolves through. A second
/// type would make every one of those two comparisons and would ask the
/// compiler's cached reads to know about both, which is a change to the
/// agreement between the runtime and the emitter for a fact neither of them
/// cares about. A sentinel in a slot the runtime already reads is a change to
/// neither.
///
/// `u64::MAX` cannot collide: a slab index is a `u32`, so every real one is
/// below 2^32.
pub(super) const IN_CELL: u64 = u64::MAX;

/// The first slot the `Str` occupies.
///
/// After slot 0 (where the text is) and [`super::TEXT_LENGTH_SLOT`] (the
/// length as a value, which an inline cache answers from and which therefore
/// cannot move).
pub(super) const FIRST_SLOT: u32 = 2;

/// How many slots the `Str` spans.
pub(super) const SLOTS: u32 = (size_of::<Str>() as u32).div_ceil(8);

/// The three facts the cast below is sound on, checked by the compiler rather
/// than believed.
///
/// A `const` block and not a test: a test says so on the machines that run it,
/// and this must stop the build everywhere. Each of the three is a thing a
/// future change to `Str` would break silently — widening `INLINE` past the
/// cell, adding a field that raises the alignment, or a layout that needs more
/// than the slots there are.
const _: () = {
    assert!(
        SLOTS <= crate::heap::INLINE_SLOTS - FIRST_SLOT,
        "a `Str` no longer fits in the slots a string cell has after slot 0 and the length"
    );
    assert!(
        align_of::<Str>() <= 8,
        "a `Str` needs more than word alignment, which a cell's payload cannot promise"
    );
    assert!(
        size_of::<Str>() % align_of::<Str>() == 0,
        "a `Str` is not a whole number of its own alignment, so a word offset cannot hold one"
    );
};

/// Puts `text` in `cell`'s own slots and says so in slot 0.
///
/// # Panics
///
/// In debug, if `text` owns heap memory — see the module's own reasoning. The
/// caller is `Context::intern_value`, which branches on exactly that
/// predicate, so a panic here means the branch and this function disagree.
///
/// # Safety
///
/// Sound rather than unsafe-to-call, and these are the reasons rather than a
/// contract for the caller:
///
/// - **The window is the right size and alignment.** The `const` block above
///   is what says so, and the region's words are `u64`, so every cell payload
///   is eight-aligned (`Region`'s own documentation on why it owns a
///   `Vec<u64>`).
/// - **The address is stable.** `Region::grow` resizes inside a reservation
///   claimed at construction and asserts the base did not move, and
///   `heap/region/growth.rs` refuses `realloc` by name. Compiled code already
///   depends on this through `base()`.
/// - **Nothing reads the window as anything else.** The layout declares these
///   slots `Repr::I64`, so the tracer skips them, and `payload_words_mut` is
///   deliberately not `set_field`, so no assertion asks whether a byte of text
///   is a reference.
pub(super) fn place(context: &mut Context, cell: u32, text: Str) {
    debug_assert!(
        text.owns_nothing(),
        "a `Str` that owns a buffer was placed in a cell, which leaks it at the next collection"
    );
    let window = context
        .region
        .payload_window(cell, FIRST_SLOT, SLOTS)
        .expect("a string cell is wide enough to hold its own text");
    // `write` and not an assignment: the window holds whatever the allocator
    // left there, which is not a `Str`, so dropping it is what an assignment
    // would do.
    unsafe { std::ptr::write(window.cast::<Str>(), text) };
    context
        .region
        .set_field(cell, 0, IN_CELL)
        .expect("a string cell has a first slot");
}

/// The `Str` in `cell`'s own slots, or `None` if its text is in the slab.
///
/// Takes the region rather than the context so that the borrow is of one
/// FIELD: `Context::key_of_text_cell` reads the text while it writes the
/// interner and the key registry, which Rust allows only because they are
/// different fields. That trick is why this is not a method on `Context`.
pub(super) fn at(region: &crate::heap::Region, cell: u32) -> Option<&Str> {
    // Slot 0 and the `Str` in ONE window, not a `field` call and then a second
    // bounds-checked window: each of those decomposes the reference and checks
    // the cell's width again, and reading a string is more frequent than making
    // one. Measured — splitting it cost 1.1 ns on `text_at`, which is a third
    // of what the whole read costs.
    let window = region.payload_window(cell, 0, FIRST_SLOT + SLOTS)?;
    // Sound for the reasons `place` states, plus the one this read adds: the
    // words are `UnsafeCell`, so a `&Str` into them may have its `Cell` memo
    // written through it — which is what `Context::key_of_text_cell` does and
    // what `Region::words` is declared `UnsafeCell` for.
    unsafe {
        if *window != IN_CELL {
            return None;
        }
        Some(&*window.add(FIRST_SLOT as usize).cast::<Str>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::current::with_context;
    use crate::value::{Kinds, Singletons};

    fn context() -> Context {
        Context::new(
            Singletons { undefined: 0, null: 1, hole: 2 },
            Kinds { symbol: 4, bigint: 5 },
        )
    }

    #[test]
    fn a_strings_own_bytes_are_never_asked_whether_they_are_a_reference() {
        // The abort this placement cost before `Repr::Payload` existed, pinned
        // so it cannot come back quietly.
        //
        // `Value::as_slot` wants the word's top sixteen bits to be exactly
        // 0xFFFA. Word 0 of an in-cell `Str` is the `Narrow` discriminant, the
        // length, then `held[0..5]` — so `held[4]` and `held[5]` land in the
        // word's top half, and `ú` (0xFA) then `ÿ` (0xFF) put the reference tag
        // there. The low thirty-two bits are then `(len << 8) | held[0] << 16 |
        // held[1] << 24`, which for four leading NULs is 0x600: cell 1536, a
        // cell most programs have. So the word decodes as a live reference, and
        // the check that refuses a non-followed field holding one fired on a
        // perfectly ordinary six-character string.
        //
        // Four strings, one per word boundary the `Str` spans.
        let (_context, ()) = with_context(context(), || {
            let hostile = [
                "\0\0\0\0\u{fa}\u{ff}",
                "\0\0\0\0\0\0\0\0\0\0\0\0\u{fa}\u{ff}",
                "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\u{fa}\u{ff}",
                "\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\u{fa}\u{ff}",
            ];
            let mut made = Vec::new();
            for text in hostile {
                let value = crate::entry::with_runtime(|context| {
                    context.intern_value(crate::text::Str::from_str(text)).bits()
                });
                made.push(value);
            }
            // The walk is what used to abort. Marking over a stack holding
            // nothing reaches every live cell, these four among them.
            crate::entry::with_runtime(|context| {
                let _ = crate::entry::trace::mark(context, &[]);
                for (value, text) in made.iter().zip(hostile) {
                    let cell = crate::value::Value(*value).as_slot().expect("a text cell");
                    assert_eq!(
                        context.text_at(cell).and_then(crate::text::Str::to_rust).as_deref(),
                        Some(text),
                        "the bytes survived a walk that once refused to look at them"
                    );
                }
            });
        });
    }

    #[test]
    fn only_a_str_that_owns_nothing_goes_in_the_cell() {
        // The invariant the placement rests on, from both sides: an inline run
        // is in the cell and takes no slab slot, a spilled one is in the slab
        // and slot 0 is an index rather than the sentinel.
        let (_context, ()) = with_context(context(), || {
            crate::entry::with_runtime(|context| {
                let short = context.intern_value(crate::text::Str::from_str("short"));
                let short_cell = short.as_slot().expect("a text cell");
                assert_eq!(
                    context.region.field(short_cell, 0),
                    Some(IN_CELL),
                    "an inline run carries its own text"
                );
                assert!(at(&context.region, short_cell).is_some());

                let spilling = "x".repeat(crate::text::INLINE + 1);
                let long = context.intern_value(crate::text::Str::from_str(&spilling));
                let long_cell = long.as_slot().expect("a text cell");
                assert_ne!(
                    context.region.field(long_cell, 0),
                    Some(IN_CELL),
                    "a spilled run keeps the slab, because a cell cannot free its buffer"
                );
                assert!(at(&context.region, long_cell).is_none());
                assert_eq!(
                    context.text_at(long_cell).and_then(crate::text::Str::to_rust),
                    Some(spilling)
                );
            });
        });
    }
}
