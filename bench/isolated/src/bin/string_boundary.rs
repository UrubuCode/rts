//! **Experiment 4 — what a string method pays before it does anything.**
//!
//! # The question
//!
//! Three rows of `bench/analytic.ts` land within 3% of each other while doing
//! wildly different amounts of work:
//!
//! | action | rts | bun | node |
//! |---|---:|---:|---:|
//! | `string indexOf 256` — a 256-unit scan | 207.45 | 0.50 | 0.39 |
//! | `string slice 16` — a 16-unit copy | 206.59 | 0.60 | 0.38 |
//! | `string toUpperCase 16` — a 16-unit case fold | 202.29 | 0.38 | 0.38 |
//!
//! A scan of 256 units, a copy of 16 and a case fold of 16 are not the same
//! amount of work, so **none of those numbers is the cost of the operation**.
//! Something fixed dominates all three.
//!
//! Reading the code says what. `crates/rts-core/src/entry/text.rs:85`, the last
//! line of `to_text`:
//!
//! ```ignore
//! // A string is its own text; anything else on the heap is an object.
//! Kind::Reference(slot) => context.text_at(slot as u32).cloned(),
//! ```
//!
//! Every string method reaches its receiver through `text_of` → `to_text`, and
//! that `.cloned()` **copies the entire receiver onto the heap before the method
//! begins**. Then the answer is built, and
//! `crates/rts-core/src/entry/context.rs:309` puts it back:
//!
//! ```ignore
//! pub fn intern_value(&mut self, text: Str) -> Value {
//!     let slot = self.cells.insert(text).slot();   // a slab insert
//!     let cell = alloc_or_die(self, STRIDE, ty);   // a 128-byte region cell
//!     ...two field writes
//! }
//! ```
//!
//! So `"abc".toUpperCase()` allocates three times: the receiver clone, the
//! result buffer, and the result's cell. `"…".indexOf("x")` returns a *number*
//! and still allocates the receiver clone, which is why a 256-unit scan and a
//! 16-unit slice cost the same.
//!
//! The argument side of exactly this was already found and fixed —
//! `crates/rts-core/src/entry/string/coerce.rs:151` records `indexOf` on 16
//! units costing 299 ns, "~210 ns of the difference was this conversion rather
//! than the search". **The receiver side was not.**
//!
//! # Why the clone is there, and why removing it is not free
//!
//! Not carelessness — the borrow checker. `to_text` takes `&Context` and the
//! caller needs `&mut Context` afterwards to allocate the answer, so a `&Str`
//! borrowed out of the slab cannot still be alive when the answer is built.
//! Cloning ends the borrow. Any fix has to end it another way, and the three
//! candidates below are the ones that do:
//!
//! - **take and put back** — `std::mem::take` the `Str` out of its slab entry,
//!   leaving an empty one, and restore it before returning. No allocation, one
//!   move each way. Costs: the slab holds a lie for the duration, so anything
//!   reachable during the call sees an empty string, and a `?` that returns
//!   early leaks the lie permanently.
//! - **compute first, then allocate** — do the whole operation against the
//!   borrow, drop it, and only then take `&mut Context`. Costs nothing at run
//!   time and costs a restructure of every method that does not already have
//!   that shape.
//! - **reference-count the text** — `Rc<Str>` in the slab, so `.cloned()`
//!   becomes a refcount bump. Costs a word per string and an increment/decrement
//!   pair, and makes the collector's job different.
//!
//! This experiment prices the *saving*, so that the restructure is only
//! attempted if the saving is worth it.
//!
//! # What is being compared
//!
//! Two groups, because the two shapes of string method have different answers.
//!
//! **Group A — a method that returns a string** (`toUpperCase` on 16 latin-1
//! units). Rows 1–4 remove one allocation each.
//!
//! **Group B — a method that returns a number** (`indexOf` on 256 units). The
//! receiver clone is the *only* allocation, so rows 5–6 price it alone, and
//! they are the cleanest reading of the 207 ns.

use rts_isolated::{measure, opaque, report};

/// A string, as `crates/rts-core/src/text/mod.rs` stores one: whichever of two
/// layouts fits, remembering which.
///
/// Only the narrow arm is exercised below. That is the case the benchmark rows
/// are in — `"abcdefghijklmnop"` is latin-1 — and it is also the case that
/// makes the clone look *cheapest*, so a saving measured here is a lower bound
/// on the saving for wide text.
#[derive(Clone)]
enum Str {
    Latin1(Vec<u8>),
    #[allow(dead_code)]
    Utf16(Vec<u16>),
}

impl Str {
    fn units(&self) -> usize {
        match self {
            Str::Latin1(bytes) => bytes.len(),
            Str::Utf16(units) => units.len(),
        }
    }

    fn bytes(&self) -> &[u8] {
        match self {
            Str::Latin1(bytes) => bytes,
            Str::Utf16(_) => &[],
        }
    }
}

impl Default for Str {
    fn default() -> Self {
        Str::Latin1(Vec::new())
    }
}

/// The slab strings live in, as `Context::cells` is: Rust values behind indices.
struct Slab {
    entries: Vec<Str>,
    free: Vec<u32>,
}

impl Slab {
    fn new() -> Self {
        Slab {
            entries: Vec::with_capacity(1024),
            free: Vec::new(),
        }
    }

    #[inline(always)]
    fn insert(&mut self, text: Str) -> u32 {
        match self.free.pop() {
            Some(slot) => {
                self.entries[slot as usize] = text;
                slot
            }
            None => {
                self.entries.push(text);
                (self.entries.len() - 1) as u32
            }
        }
    }

    #[inline(always)]
    fn at(&self, slot: u32) -> &Str {
        &self.entries[slot as usize]
    }

    /// `.cloned()`, as `to_text` performs it.
    #[inline(always)]
    fn cloned(&self, slot: u32) -> Str {
        self.entries[slot as usize].clone()
    }

    /// The first candidate fix: move the value out, leaving an empty one.
    #[inline(always)]
    fn take(&mut self, slot: u32) -> Str {
        std::mem::take(&mut self.entries[slot as usize])
    }

    #[inline(always)]
    fn put(&mut self, slot: u32, text: Str) {
        self.entries[slot as usize] = text;
    }

    /// Returned to the free list, so a long run does not grow without bound and
    /// the measurement stays about the call rather than about a growing `Vec`.
    #[inline(always)]
    fn release(&mut self, slot: u32) {
        self.entries[slot as usize] = Str::default();
        self.free.push(slot);
    }
}

/// The cell a string value names, as `intern_value` builds one: a header, the
/// slab position, and the length as a value.
const CELL_WORDS: usize = 16;

struct Region {
    words: Vec<u64>,
    next: usize,
}

impl Region {
    fn new(cells: usize) -> Self {
        Region {
            words: vec![0; cells * CELL_WORDS],
            next: 0,
        }
    }

    #[inline(always)]
    fn alloc(&mut self, type_id: u32) -> usize {
        if self.next + CELL_WORDS > self.words.len() {
            self.next = 0;
        }
        let cell = self.next;
        self.next += CELL_WORDS;
        self.words[cell] = (type_id as u64) << 32;
        cell
    }
}

/// `intern_value`: slab insert, region cell, two field writes.
#[inline(always)]
fn intern_value(slab: &mut Slab, region: &mut Region, text: Str) -> usize {
    let length = text.units();
    let slot = slab.insert(text);
    let cell = region.alloc(7);
    region.words[cell + 1] = slot as u64;
    region.words[cell + 2] = (length as f64).to_bits();
    cell
}

/// The case fold, over the narrow form. `u8::to_ascii_uppercase`, as
/// `crates/rts-core/src/entry/string/basic.rs:272` passes it.
#[inline(always)]
fn upper(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().map(|b| b.to_ascii_uppercase()).collect()
}

/// The scan `indexOf` performs.
#[inline(always)]
fn find(haystack: &[u8], needle: &[u8]) -> i64 {
    if needle.is_empty() || needle.len() > haystack.len() {
        return -1;
    }
    for start in 0..=haystack.len() - needle.len() {
        if &haystack[start..start + needle.len()] == needle {
            return start as i64;
        }
    }
    -1
}

fn main() {
    let mut slab = Slab::new();
    let mut region = Region::new(8192);

    // `"abcdefghijklmnop"`, the receiver of the 16-unit rows.
    let short = slab.insert(Str::Latin1((b'a'..=b'p').collect()));
    // The same, repeated sixteen times: the 256-unit receiver.
    let long = slab.insert(Str::Latin1(
        (b'a'..=b'p').cycle().take(256).collect::<Vec<u8>>(),
    ));
    let needle: Vec<u8> = b"mnop".to_vec();

    let rows = vec![
        // ------------------------------------------------- group A, row 1
        measure("A1. toUpperCase: clone + map + intern (today)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let text = slab.cloned(opaque(short));
                let folded = upper(text.bytes());
                let cell = intern_value(&mut slab, &mut region, Str::Latin1(folded));
                acc = acc.wrapping_add(region.words[cell + 1]);
                slab.release(region.words[cell + 1] as u32);
            }
            acc
        }),
        // ------------------------------------------------- group A, row 2
        measure("A2.  + take/put instead of clone", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let text = slab.take(opaque(short));
                let folded = upper(text.bytes());
                slab.put(short, text);
                let cell = intern_value(&mut slab, &mut region, Str::Latin1(folded));
                acc = acc.wrapping_add(region.words[cell + 1]);
                slab.release(region.words[cell + 1] as u32);
            }
            acc
        }),
        // ------------------------------------------------- group A, row 3
        measure("A3.  + borrow, no move at all", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                // The "compute first, then allocate" shape: the whole operation
                // runs against a borrow, which ends before the answer is built.
                let folded = upper(slab.at(opaque(short)).bytes());
                let cell = intern_value(&mut slab, &mut region, Str::Latin1(folded));
                acc = acc.wrapping_add(region.words[cell + 1]);
                slab.release(region.words[cell + 1] as u32);
            }
            acc
        }),
        // ------------------------------------------------- group A, row 4
        measure("A4.  + the fold alone (the floor)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let folded = upper(slab.at(opaque(short)).bytes());
                acc = acc.wrapping_add(folded[0] as u64);
            }
            acc
        }),
        // ------------------------------------------------- group B, row 5
        measure("B5. indexOf 256: clone + scan (today)", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                let text = slab.cloned(opaque(long));
                acc = acc.wrapping_add(find(text.bytes(), &needle) as u64);
            }
            acc
        }),
        // ------------------------------------------------- group B, row 6
        measure("B6.  + borrow instead of clone", |n| {
            let mut acc = 0u64;
            for _ in 0..n {
                acc = acc.wrapping_add(find(slab.at(opaque(long)).bytes(), &needle) as u64);
            }
            acc
        }),
    ];

    report("Experiment 4 - what a string method pays before it works", &rows);
    println!();
    println!("Group A is a method that answers a string; group B one that answers a");
    println!("number, where the receiver clone is the only allocation. B5 minus B6 is");
    println!("the cleanest price of `to_text`'s `.cloned()` there is.");
}
