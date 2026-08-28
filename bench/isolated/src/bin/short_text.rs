//! **Experiment 10 — what the separate heap buffer costs a short string.**
//!
//! # The question
//!
//! Every string this engine makes costs a region cell AND a `Vec`. `Str` is
//! `crates/rts-core/src/text/mod.rs`:
//!
//! ```ignore
//! pub enum Repr {
//!     Latin1(Vec<u8>),
//!     Utf16(Vec<u16>),
//! }
//! ```
//!
//! and `Context::intern_value` (`entry/context.rs:493`) puts that `Str` in a
//! slab beside a region cell. So making `"abc"` is: one region allocation, one
//! slab insert, two `set_field`s — and, inside the `Str`, **one `malloc` for the
//! text itself**.
//!
//! Measured in the engine 2026-08-28, release, on an idle machine:
//!
//! | | ns |
//! |---|---:|
//! | `new Callee()` — a plain object, one region cell | 75.9 |
//! | `String(7)` — a one-character string | 126.8 |
//! | `s256.slice(0, 1)` — a one-character string | 154.9 |
//! | `"a,b,…,h".split(",")` — eight of them | 1286.7 |
//!
//! An object is 76 ns and a one-character string is 127-155. **Something
//! string-shaped costs 50-80 ns on top of the allocation an object already
//! pays**, and the separate buffer is the candidate: it is the one thing a
//! string does that an object does not.
//!
//! `split` is the row that makes this worth asking. Its cost is ~110 ns per
//! piece, which is not the splitting — it is making the pieces.
//!
//! # What is being compared
//!
//! Text is put into a slab exactly as the engine does it, and only the
//! representation differs:
//!
//! 1. **`Vec<u8>` — the engine today.** One heap allocation per string.
//! 2. **Inline up to 22 bytes, heap beyond.** The `Vec` is replaced by a fixed
//!    array plus a length, so a short string allocates nothing of its own. 22 is
//!    what fits beside a length byte in the 24 bytes a `Vec<u8>` already
//!    occupies, so the enum does not grow.
//! 3. **The heap buffer, freed as well as made.** A string is collected, and
//!    what the collector gives back is this `free`. Shape 1 measures only the
//!    making; a fair account of what shape 2 removes has to include it.
//! 4. **Neither — the slab insert alone**, as the floor, so the rows above are
//!    read as "what the text costs on top of the bookkeeping".
//!
//! Lengths of 1, 8 and 32 bytes, because the answer is only interesting where
//! the text is short — 32 is past the inline bound on purpose, so the table
//! shows the case the change does NOT help.
//!
//! # RESULT
//!
//! ~25 ns for a short string, and the past-the-bound row flat. The engine then
//! REFUSED the change: `docs/codegen/native-call-floor.md` §6 has the whole
//! account, and the short version is that this experiment cannot see the
//! second match every text READ would have to make, which costs more than the
//! `malloc` this saves. Kept because the 25 ns is real and so is the flat
//! fallback; what it does not say is what README rule 2 says it cannot.

use rts_isolated::{measure, opaque, report};

/// What fits beside a length in the 24 bytes a `Vec<u8>` already takes.
const INLINE: usize = 22;

/// The engine's shape: the text is always a separate allocation.
enum Heaped {
    Latin1(Vec<u8>),
}

/// The proposal: short text lives in the value, long text on the heap.
enum Inlined {
    Short { bytes: [u8; INLINE], len: u8 },
    Latin1(Vec<u8>),
}

impl Inlined {
    fn new(source: &[u8]) -> Self {
        if source.len() <= INLINE {
            let mut bytes = [0u8; INLINE];
            bytes[..source.len()].copy_from_slice(source);
            return Self::Short {
                bytes,
                len: source.len() as u8,
            };
        }
        Self::Latin1(source.to_vec())
    }

    fn len(&self) -> usize {
        match self {
            Self::Short { len, .. } => *len as usize,
            Self::Latin1(bytes) => bytes.len(),
        }
    }
}

/// A slab entry, so the comparison includes the move the engine performs.
fn drive_heaped(source: &[u8], n: u64) -> u64 {
    let mut slab: Vec<Heaped> = Vec::with_capacity(64);
    let mut sink = 0u64;
    for _ in 0..n {
        let made = Heaped::Latin1(opaque(source).to_vec());
        let Heaped::Latin1(bytes) = &made;
        sink = sink.wrapping_add(bytes.len() as u64);
        slab.push(made);
        // Popped rather than grown without bound, which is what a collected
        // heap does: the entry is handed back and the buffer with it.
        slab.pop();
    }
    sink
}

fn drive_inlined(source: &[u8], n: u64) -> u64 {
    let mut slab: Vec<Inlined> = Vec::with_capacity(64);
    let mut sink = 0u64;
    for _ in 0..n {
        let made = Inlined::new(opaque(source));
        sink = sink.wrapping_add(made.len() as u64);
        slab.push(made);
        slab.pop();
    }
    sink
}

/// The bookkeeping with no text at all: the floor both rows sit on.
fn drive_bare(source: &[u8], n: u64) -> u64 {
    let mut slab: Vec<usize> = Vec::with_capacity(64);
    let mut sink = 0u64;
    for _ in 0..n {
        let made = opaque(source).len();
        sink = sink.wrapping_add(made as u64);
        slab.push(made);
        slab.pop();
    }
    sink
}

fn main() {
    let one = vec![b'a'; 1];
    let eight = vec![b'a'; 8];
    let thirty_two = vec![b'a'; 32];

    let rows = [
        measure("1 byte  - Vec (engine today)", |n| drive_heaped(&one, n)),
        measure("1 byte  - inline", |n| drive_inlined(&one, n)),
        measure("8 bytes - Vec (engine today)", |n| drive_heaped(&eight, n)),
        measure("8 bytes - inline", |n| drive_inlined(&eight, n)),
        measure("32 bytes - Vec (engine today)", |n| {
            drive_heaped(&thirty_two, n)
        }),
        measure("32 bytes - inline (past bound)", |n| {
            drive_inlined(&thirty_two, n)
        }),
        measure("floor - slab move, no text", |n| drive_bare(&eight, n)),
    ];

    report("Experiment 10 - the short string's own buffer", &rows);
}
