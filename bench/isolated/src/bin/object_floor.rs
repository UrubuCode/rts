//! **Experiment 8 — the floor: what would an object cost at machine potential?**
//!
//! # The question
//!
//! `bench/analytic.ts` measures `alloc class instance` at **90.89 ns** against
//! 0.53 on bun and 0.38 on node, and the standing question is whether to keep,
//! mend or replace the Hermes-shaped object model. Nobody has said what
//! "close to the machine" would COST here, so nobody can say how much of the
//! 90.89 is the model and how much is work that merely happens to sit around
//! it. This experiment establishes the number the rest of that argument is
//! measured against.
//!
//! It is not a re-run of experiment 3. That one priced the *by-name* write path
//! and found, correctly, that shape transitions are memoised twice over and
//! account for **none** of the 90.9 ns
//! (`crates/rts-cranelift/src/shape/tree.rs:136`, `crates/rts-core/src/entry/cache.rs`;
//! `RTS_CACHE_CENSUS=1` reports 0 misses and 0 sites over 200 000 `new Callee()`).
//! This one prices the parts that are NOT the shape tree: the cell layout, the
//! zero fill on reuse, the prototype-chain read, the `typed_as` find, the side
//! tables a sweep has to clear, and the entry-point call itself.
//!
//! # The machine is loaded — treat these as indicative
//!
//! Written and first run on 2026-08-22 with several agents working in the same
//! checkout. The harness takes the best of three and the best is the closest
//! thing to the code's own cost, but a loaded machine can only make a run
//! slower. **Re-run this quiet before quoting an absolute number**; the ratios
//! between rows are the durable part, since every row pays the same tax.
//!
//! # What is being modelled, and against what
//!
//! Every row allocates out of a region the same size the host actually gives a
//! program — `crates/rts-host/src/run.rs:1095` says `CELLS = 1 << 16`, and
//! `crates/rts-core/src/heap/region/mod.rs:181` says fifteen inline slots, so
//! the region is 65 536 × 128 B = **8 MB**. That is deliberate and it is the
//! part a smaller model would get wrong: 8 MB does not fit in any cache on this
//! machine, so a cell handed back by the free list is **cold**, and what it
//! costs to write 128 bytes into a cold cell is exactly the question rows 4–6
//! exist to answer.
//!
//! `RTS_GC_DEBUG=1` over 200 000 `new Callee()` on `target/release/rts.exe`
//! prints three cycles, each `live 1796 freed 63667` — so 191 001 cells are
//! recycled for 200 000 allocations. **In steady state every `new` is served by
//! the free list and pays for one `release`.** Rows 4–6 and 12–13 are that
//! steady state; rows 1–2 are the bump path, which a program only sees for its
//! first 65 536 objects.
//!
//! # The rows
//!
//! 1–3 are the floor. 4–6 are what reuse costs, and their spread is the price
//! of the zero fill at `region/mod.rs:485`. 7–13 are the per-object bookkeeping
//! `allocate_for_target` and `collect_cycle::release` perform, each isolated.
//! 14–15 put the pieces back together.
//!
//! # RESULT, 2026-08-22, loaded machine, min of five runs
//!
//! | | ns/op |
//! |---|---:|
//! | 0a. HOT 128 B cell: header + store + read | **1.66** |
//! | 0b. HOT reuse + 15 zero words | 7.83 |
//! | 0c. HOT `own_property` probe | 10.30 |
//! | 0d. HOT `well_known("prototype")` scan | 1.97 |
//! | 1. bump 128 B cell, 8 MB region | **3.65** |
//! | 2. bump 64 B cell (7 slots) | 1.79 |
//! | 3. plain Rust struct into a `Vec` | **2.10** |
//! | 4. free-list reuse + 15 zero words (engine) | 8.03 |
//! | 5. free-list reuse + 7 zero words | 4.25 |
//! | 6. free-list reuse, NO zero fill | 3.17 |
//! | 7. + prototype write | 4.55 |
//! | 8. + `own_property`, one link | 10.54 |
//! | 9. + chain read, two links | 20.72 |
//! | 10. + `typed_as` find over a list of 1 | 4.28 |
//! | 11. + `typed_as` find over a list of 32 | 18.93 |
//! | 12. + 26 `Aside::remove`, tables empty | 15.00 |
//! | 13. + 26 `Aside::remove`, tables grown | 154.19 |
//! | 14. `allocate_for_target` modelled | 20.58 |
//! | 15. the same behind `extern "C"` | 22.07 |
//!
//! Differences taken WITHIN each run rather than between the minima, because
//! the cold rows share a base that moved between 3.65 and 15.07 while other
//! agents were building:
//!
//! | | ns, over five runs |
//! |---|---:|
//! | the fifteen zero words, cell in cache (0b−0a) | 5.9 – 8.5 |
//! | the fifteen zero words, cell cold (4−6) | 4.1 – 5.4 |
//! | one `own_property`, in cache (0c−0a) | 8.5 – 12.7 |
//! | `set_prototype`'s `Aside` write (7−1) | 1.5 – 1.8 |
//! | `typed_as` over a list of one (10−1) | 0.7 – 1.1 |
//! | 26 `Aside::remove` over EMPTY tables (12−1) | 9.3 – 12.5 |
//! | 26 `Aside::remove` over GROWN tables (13−1) | 145 – 165 |
//!
//! **Three things fall out of that.**
//!
//! The **floor is 1.7 ns in cache and 3.7 ns over the region a program really
//! gets**, and a plain Rust struct on the same machine is 2.1 ns. RTS's cell —
//! a header word, a tagged field, an index-not-address reference — costs
//! essentially nothing over a native one. Whatever the 90.89 ns is, it is not
//! the layout.
//!
//! The **zero fill is worth about six nanoseconds per object** and it is
//! instruction-bound rather than bandwidth-bound: rows 0b−0a (everything in L1)
//! and rows 4−6 (an 8 MB working set) agree within a nanosecond and a half, and
//! halving the slot count halves it (row 5). So moving *when* it happens buys
//! nothing and shrinking *how many words* it writes buys all of it.
//!
//! The **side tables a sweep clears are the widest bracket in the table.**
//! Twenty-six `Aside::remove` calls cost 9–13 ns when the tables were never
//! grown and 145–165 ns when every one of them spans the region, because at
//! that size each is a separate 1 MB stream and twenty-six streams is more than
//! the prefetcher tracks. Which end a program sits at is decided by how many of
//! the twenty-six were ever written at a HIGH cell index — and for
//! `new Callee()` that is one, `prototypes`, written by `functions.rs:912` on
//! every construction.
//!
//! # What this deliberately does not model
//!
//! The constructor call and its frame, the field initialiser, `with_current`
//! (priced separately at 0.53 ns by experiment 1), and the mark phase — 1 796
//! live cells over 200 000 allocations is 0.027 marks per object and rounds to
//! nothing. A row here is a lower bound on what removing that work would save,
//! not the saving.

use rts_isolated::{measure, opaque, report};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The hasher `crates/rts-cranelift/src/shape/tree.rs` uses, transcribed.
///
/// Transcribed rather than imported because a `[[bin]]` cannot see another
/// `[[bin]]`, and the alternative — hoisting it into `src/lib.rs` — would
/// recompile `object_new.rs`, whose recorded numbers are in its own module
/// documentation and must not be perturbed by a change that is not about it.
/// Substituting `std`'s default hasher instead would measure SipHash and report
/// a cost the engine does not pay: `tree.rs` uses `rustc_hash::FxHashMap`
/// because its keys are numbers the crate mints itself.
#[derive(Default)]
struct FxHasher {
    hash: u64,
}

impl FxHasher {
    const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(Self::SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.add(*byte as u64);
        }
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(value as u64);
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// How many cells the host gives a program — `crates/rts-host/src/run.rs:1095`.
const CELLS: usize = 1 << 16;

/// A region small enough to stay in cache, for the two rows that separate the
/// INSTRUCTIONS from the memory system.
///
/// 512 cells × 128 B = 64 KB. Everything a row touches at this size is L1/L2
/// resident, so the number is the cost of the code and nothing else. The
/// difference against the same row at [`CELLS`] is what an 8 MB working set
/// costs — and that difference is the part a loaded machine moves, which is why
/// both are here.
const HOT_CELLS: usize = 512;

/// Inline slots per cell — `crates/rts-core/src/heap/region/mod.rs:181`.
const WIDE_SLOTS: usize = 15;

/// What `INLINE_SLOTS` was before 2026-08-11, kept as a row because the region's
/// own documentation records that the change cost `bench/objbench.ts` 23.5%.
const NARROW_SLOTS: usize = 7;

/// Words per cell, header included.
const WIDE_WORDS: usize = 1 + WIDE_SLOTS;
const NARROW_WORDS: usize = 1 + NARROW_SLOTS;

/// A header word out of its two halves — `region/mod.rs:277`.
#[inline(always)]
fn header_word(ty: u32, width: u32) -> u64 {
    ((width as u64) << 32) | ty as u64
}

/// A stand-in for `Aside<T>`: `Vec<Option<T>>` indexed by cell, which is
/// literally what `crates/rts-core/src/heap/aside.rs:52` holds.
type Aside<T> = Vec<Option<T>>;

/// How many side tables `collect_cycle::release` clears per freed cell.
///
/// Counted at `crates/rts-core/src/entry/collect_cycle.rs:151-230`: `spill_of`,
/// `array_elements`, `buffer_of`, `detached`, `prototypes`, `proto_types`,
/// `callables`, `cursors`, `bound`, `views`, `collections`, `generators` and
/// the rest of the field list, plus `weak::clear_freed` and
/// `finalize::queue_freed`. Twenty-six is the figure the investigation this
/// experiment serves was given; the two rows below bracket it rather than
/// resting on it, because what a `remove` costs depends entirely on whether the
/// table was ever grown.
const SIDE_TABLES: usize = 26;

/// How many layouts a real program's shape tree holds, for the map rows.
///
/// `bench/analytic.ts` alone builds a shape for every object literal, every
/// class, every closure environment and every module scope in the file.
const LAYOUTS: u32 = 2000;

fn main() {
    // 8 MB, the size the host actually gives a program.
    let mut wide = vec![0u64; CELLS * WIDE_WORDS];
    let mut narrow = vec![0u64; CELLS * NARROW_WORDS];
    // 64 KB, so the same code can be measured with the memory system out of it.
    let mut hot = vec![0u64; HOT_CELLS * WIDE_WORDS];

    // `ShapeTree::indexes`, as `tree.rs:280` builds it: a map from a layout to a
    // map from a key to a slot. `slot_of` costs THREE probes over these two —
    // `contains_key`, then `Index`, then `get` on the inner map.
    let mut indexes: FxMap<u32, FxMap<u32, u32>> = FxMap::default();
    for layout in 0..LAYOUTS {
        let mut inner: FxMap<u32, u32> = FxMap::default();
        for property in 0..4u32 {
            inner.insert(layout * 8 + property, property);
        }
        indexes.insert(layout, inner);
    }

    // `Context::shape_of` — a `Vec<ShapeId>` indexed by the type number
    // (`crates/rts-core/src/entry/context.rs:152`).
    let shape_of_type: Vec<u32> = (0..LAYOUTS).collect();

    // `Context::proto_types`, an `Aside<Vec<(ShapeId, TypeId)>>`
    // (`crates/rts-core/src/entry/mod.rs:343`). One list of length 1 — which is
    // what `functions.rs:906` says a plain class reaches — and one of length 32,
    // to show what a crowded prototype would cost.
    let short_list: Vec<(u32, u32)> = vec![(7, 7)];
    let long_list: Vec<(u32, u32)> = (0..32u32).map(|at| (at, at)).collect();

    // `Context::prototypes`, grown to the whole region because
    // `allocate_for_target` writes one entry per `new`.
    let mut prototypes: Aside<u64> = vec![None; CELLS];

    // The side tables a sweep clears. Two sets: one never grown — which is what
    // most of the twenty-six are in a program that makes plain objects — and one
    // grown to the whole region, which is what `prototypes` genuinely is.
    let mut empty_tables: Vec<Aside<u64>> = (0..SIDE_TABLES).map(|_| Vec::new()).collect();
    let mut grown_tables: Vec<Aside<u64>> =
        (0..SIDE_TABLES).map(|_| vec![Some(1u64); CELLS]).collect();

    // A plain Rust instance, for row 3.
    struct Instance {
        v: f64,
    }
    let mut instances: Vec<Instance> = Vec::with_capacity(CELLS);

    let rows = vec![
        // ---------------------------------------------------------------- 0
        // The instruction floor: the same code as row 1 over a 64 KB region, so
        // every cell is already in cache. Nothing about RTS's object model can
        // cost less than this, and unlike row 1 it does not move when another
        // process is using the memory bus.
        measure("0a. HOT 128B cell: header + store + read", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > hot.len() {
                    at = 0;
                }
                hot[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                hot[at + 1] = opaque(i);
                acc = acc.wrapping_add(hot[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // --------------------------------------------------------------- 0b
        // The zero fill with the memory system taken out: fifteen stores into a
        // cell that is already in L1. Row 4 minus row 6 is what the fill costs
        // when the cell is cold; this minus row 0a is what its INSTRUCTIONS
        // cost, and the gap between those two answers is the whole point.
        measure("0b. HOT reuse + 15 zero words", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > hot.len() {
                    at = 0;
                }
                let link = hot[at + 1];
                acc = acc.wrapping_add(link & 1);
                hot[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                for slot in 0..WIDE_SLOTS {
                    hot[at + 1 + slot] = 0;
                }
                hot[at + 1] = opaque(i);
                acc = acc.wrapping_add(hot[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // --------------------------------------------------------------- 0c
        // One `own_property` with every table it touches in cache, so the row
        // says what three FxHashMap probes and four loads cost as CODE.
        measure("0c. HOT own_property probe", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > hot.len() {
                    at = 0;
                }
                hot[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                hot[at + 1] = opaque(i);
                acc = acc.wrapping_add(own_property(
                    &hot,
                    at,
                    &indexes,
                    &shape_of_type,
                    opaque(3),
                ));
                acc = acc.wrapping_add(hot[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // --------------------------------------------------------------- 0d
        // `Context::well_known("prototype")` — `CACHED_KEYS.iter().position`
        // over six `&str`s (`crates/rts-core/src/entry/mod.rs:214`,
        // `context.rs:380`), then an `Option` read out of a fixed array.
        // "prototype" is the SECOND entry, so the scan compares two names and
        // the first fails on length.
        measure("0d. HOT well_known(\"prototype\") scan", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(well_known(opaque("prototype")) as u64);
                acc = acc.wrapping_add(i & 1);
            }
            acc
        }),
        // ---------------------------------------------------------------- 1
        // The bump path: no cell is reused, so nothing has to be cleared. One
        // header store, one field store, one field read. This is the floor for
        // RTS's cell layout, and the only thing below it is not having a cell.
        measure("1. bump 128B cell: header + store + read", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // ---------------------------------------------------------------- 2
        // The same at the cell size the region had before 2026-08-11. Half the
        // memory traffic per allocation, and one cache line per cell instead of
        // two.
        measure("2. bump 64B cell (7 slots)", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + NARROW_WORDS > narrow.len() {
                    at = 0;
                }
                narrow[at] = header_word(opaque(7), NARROW_SLOTS as u32);
                narrow[at + 1] = opaque(i);
                acc = acc.wrapping_add(narrow[at + 1]);
                at += NARROW_WORDS;
            }
            acc
        }),
        // ---------------------------------------------------------------- 3
        // What a machine with no object model at all pays for `class Callee { v = 1 }`:
        // eight bytes, appended. No header, no tag, no side table, no collector.
        measure("3. plain Rust struct into a Vec", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                if instances.len() == CELLS {
                    instances.clear();
                }
                instances.push(Instance {
                    v: opaque(i) as f64,
                });
                acc = acc.wrapping_add(instances[instances.len() - 1].v as u64);
            }
            acc
        }),
        // ---------------------------------------------------------------- 4
        // The steady state: a cell comes back off the free list, its link is
        // read out of slot 0, and **every one of the fifteen slots is zeroed**
        // (`region/mod.rs:481-487`). 128 bytes written per object, over an 8 MB
        // region that fits in no cache.
        measure("4. free-list reuse + 15 zero words (engine)", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                let link = wide[at + 1];
                acc = acc.wrapping_add(link & 1);
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                for slot in 0..WIDE_SLOTS {
                    wide[at + 1 + slot] = 0;
                }
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // ---------------------------------------------------------------- 5
        measure("5. free-list reuse + 7 zero words", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + NARROW_WORDS > narrow.len() {
                    at = 0;
                }
                let link = narrow[at + 1];
                acc = acc.wrapping_add(link & 1);
                narrow[at] = header_word(opaque(7), NARROW_SLOTS as u32);
                for slot in 0..NARROW_SLOTS {
                    narrow[at + 1 + slot] = 0;
                }
                narrow[at + 1] = opaque(i);
                acc = acc.wrapping_add(narrow[at + 1]);
                at += NARROW_WORDS;
            }
            acc
        }),
        // ---------------------------------------------------------------- 6
        // The same cell, reused, with the zero fill removed. Row 4 minus this
        // row is what `region/mod.rs:485` costs per object — and question 3 of
        // this investigation is whether anything reads those slots before they
        // are written.
        measure("6. free-list reuse, NO zero fill", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                let link = wide[at + 1];
                acc = acc.wrapping_add(link & 1);
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // ---------------------------------------------------------------- 7
        // `Context::set_prototype` — `prototypes.set(cell, value)`
        // (`context.rs:190`), a bounds check and a store into a 1 MB
        // `Vec<Option<u64>>` that the engine writes on **every** `new`.
        measure("7. floor + prototype write (Aside<u64> set)", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            let mut cell = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                    cell = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                prototypes[cell] = Some(opaque(i));
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
                cell += 1;
            }
            acc
        }),
        // ---------------------------------------------------------------- 8
        // One `own_property` (`objects.rs:619`): a header load for `type_of`, a
        // `Vec` index for `shape_of`, then `slot_of` — `contains_key`, `Index`,
        // and `get` on the inner map (`tree.rs:212`, `:280-291`) — then
        // `slot_value`, which calls `owned_slots` and reads the header **again**
        // (`objects.rs:942`, `:963`).
        measure("8. floor + own_property probe (1 chain link)", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(own_property(
                    &wide,
                    at,
                    &indexes,
                    &shape_of_type,
                    opaque(3),
                ));
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // ---------------------------------------------------------------- 9
        // The same walked twice, which is what a class whose `prototype` is not
        // an own property costs — and the shape the task asked to be priced.
        measure("9. floor + chain read, two links", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(own_property(
                    &wide,
                    at,
                    &indexes,
                    &shape_of_type,
                    opaque(3),
                ));
                acc = acc.wrapping_add(own_property(
                    &wide,
                    at,
                    &indexes,
                    &shape_of_type,
                    opaque(2),
                ));
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // --------------------------------------------------------------- 10
        // `Context::typed_as` (`context.rs:71-75`): an `Aside` read followed by
        // `known.iter().find`. A plain class reaches a list of ONE — the file's
        // own comment says "a scan of a list of one" — so this row is the
        // engine's real case and row 11 is the pathological one.
        measure("10. floor + typed_as find, list of 1", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(typed_as(&short_list, opaque(7)));
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // --------------------------------------------------------------- 11
        measure("11. floor + typed_as find, list of 32", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(typed_as(&long_list, opaque(31)));
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
            }
            acc
        }),
        // --------------------------------------------------------------- 12
        // `collect_cycle::release` over twenty-six tables NONE of which was ever
        // grown — `Aside::remove` is `entries.get_mut(cell)?.take()`
        // (`aside.rs:150`), so an empty table answers `None` off a length load
        // and touches no memory. This is the cheap end of the bracket.
        measure("12. floor + 26 Aside::remove, tables empty", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            let mut cell = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                    cell = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                for table in empty_tables.iter_mut() {
                    if let Some(entry) = table.get_mut(opaque(cell)) {
                        if let Some(held) = entry.take() {
                            acc = acc.wrapping_add(held);
                        }
                    }
                }
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
                cell += 1;
            }
            acc
        }),
        // --------------------------------------------------------------- 13
        // The same with every table grown to the whole region: twenty-six
        // separate 1 MB `Vec<Option<u64>>`s, each read and written at the same
        // ascending index. Twenty-six streams is more than the hardware
        // prefetcher tracks, so this is the expensive end of the bracket.
        measure("13. floor + 26 Aside::remove, tables grown", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            let mut cell = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                    cell = 0;
                }
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                wide[at + 1] = opaque(i);
                for table in grown_tables.iter_mut() {
                    if let Some(entry) = table.get_mut(opaque(cell)) {
                        // Put back, so a second pass over the region measures
                        // the same work rather than a table of `None`.
                        if let Some(held) = entry.take() {
                            acc = acc.wrapping_add(held);
                        }
                        *entry = Some(1);
                    }
                }
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
                cell += 1;
            }
            acc
        }),
        // --------------------------------------------------------------- 14
        // `allocate_for_target` (`functions.rs:864-909`) with the pieces put
        // back together, minus the entry-point call: a free-list cell with its
        // zero fill, one chain link, one `typed_as` find of a list of one, and
        // the prototype write.
        measure("14. allocate_for_target modelled", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            let mut cell = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                    cell = 0;
                }
                acc = acc.wrapping_add(own_property(
                    &wide,
                    at,
                    &indexes,
                    &shape_of_type,
                    opaque(3),
                ));
                acc = acc.wrapping_add(typed_as(&short_list, opaque(7)));
                let link = wide[at + 1];
                acc = acc.wrapping_add(link & 1);
                wide[at] = header_word(opaque(7), WIDE_SLOTS as u32);
                for slot in 0..WIDE_SLOTS {
                    wide[at + 1 + slot] = 0;
                }
                prototypes[cell] = Some(opaque(i));
                wide[at + 1] = opaque(i);
                acc = acc.wrapping_add(wide[at + 1]);
                at += WIDE_WORDS;
                cell += 1;
            }
            acc
        }),
        // --------------------------------------------------------------- 15
        // The same behind the boundary an entry point actually sits behind:
        // `#[inline(never)] extern "C"`, which the optimiser cannot see into and
        // which clobbers the caller's registers. Experiment 1 priced that call
        // and its loop at 1.19 ns with nothing inside it.
        measure("15. allocate_for_target modelled, behind extern C", |n| {
            let mut acc = 0u64;
            let mut at = 0usize;
            let mut cell = 0usize;
            for i in 0..n {
                if at + WIDE_WORDS > wide.len() {
                    at = 0;
                    cell = 0;
                }
                acc = acc.wrapping_add(across_the_boundary(
                    &mut wide,
                    at,
                    &indexes,
                    &shape_of_type,
                    &short_list,
                    &mut prototypes,
                    cell,
                    i,
                ));
                at += WIDE_WORDS;
                cell += 1;
            }
            acc
        }),
    ];

    report("Experiment 8 - the floor for an object", &rows);
    println!();
    println!("Rows 0a-0c are the same code over a 64 KB region: the INSTRUCTIONS,");
    println!("with the memory system out of it. Rows 1-15 are the same code over");
    println!("the 8 MB region a program actually gets, so each of them carries the");
    println!("memory traffic too. Row 0a is the base every ratio is against.");
    println!();
    println!("Row 1 is the floor for RTS's cell layout; row 3 is the floor for a");
    println!("machine with no object model at all. Row 4 minus row 6 is the zero");
    println!("fill at region/mod.rs:485 when the cell is cold; row 0b minus row 0a");
    println!("is the same fill when it is not. Rows 7-13 are per-object bookkeeping,");
    println!("each measured against row 1, so subtract row 1 before adding them up.");
    println!("Row 15 is the whole modelled path across an entry-point boundary.");
}

/// One `own_property`, as `crates/rts-core/src/entry/objects.rs:619` performs it.
///
/// Not inlined into the caller's loop, because in the engine it is reached
/// through a chain walk the optimiser cannot flatten either.
#[inline(never)]
fn own_property(
    words: &[u64],
    at: usize,
    indexes: &FxMap<u32, FxMap<u32, u32>>,
    shape_of_type: &[u32],
    key: u32,
) -> u64 {
    // `region.type_of` — the header, as a load.
    let ty = words[at] as u32;
    // `Context::shape_of` — a `Vec<ShapeId>` index.
    let Some(&shape) = shape_of_type.get((ty % LAYOUTS) as usize) else {
        return 0;
    };
    // `ShapeTree::slot_of` -> `index_of`: `contains_key`, then `Index`, then
    // `get` on the inner map. Three probes, written the way `tree.rs` writes
    // them rather than folded into one, because the fold is a change to the
    // engine and this is a measurement of the engine as it stands.
    if !indexes.contains_key(&shape) {
        return 0;
    }
    let inner = &indexes[&shape];
    let Some(&slot) = inner.get(&(shape * 8 + key)) else {
        return 0;
    };
    // `slot_value` -> `owned_slots`: the header again, for the width, and
    // `shape_of` again for whether the last slot is an overflow address.
    let width = (words[at] >> 32) as u32;
    let owned = if shape_of_type.get((ty % LAYOUTS) as usize).is_some() {
        width.saturating_sub(1)
    } else {
        width
    };
    if slot >= owned {
        return 0;
    }
    words[at + 1 + slot as usize]
}

/// The names `crates/rts-core/src/entry/mod.rs:214` caches, transcribed.
const CACHED_KEYS: [&str; 6] = [
    "length",
    "prototype",
    "byteLength",
    "byteOffset",
    "buffer",
    "toJSON",
];

/// The remembered key numbers `Context::well_known_keys` holds.
static WELL_KNOWN_KEYS: [Option<u32>; 6] = [
    Some(11),
    Some(12),
    Some(13),
    Some(14),
    Some(15),
    Some(16),
];

/// `Context::well_known`, as `crates/rts-core/src/entry/context.rs:366-392`
/// performs it on the path that hits.
#[inline(never)]
fn well_known(name: &str) -> u32 {
    match CACHED_KEYS.iter().position(|&known| known == name) {
        Some(at) => WELL_KNOWN_KEYS[at].unwrap_or(0),
        None => 0,
    }
}

/// `Context::typed_as`, as `crates/rts-core/src/entry/context.rs:71-75` performs
/// it: an `Aside` read, then a linear `find` over the layouts minted against
/// this prototype.
#[inline(never)]
fn typed_as(known: &[(u32, u32)], shape: u32) -> u64 {
    match known.iter().find(|(at, _)| *at == shape) {
        Some((_, ty)) => *ty as u64,
        None => 0,
    }
}

/// The whole modelled path, behind the boundary an entry point sits behind.
///
/// `extern "C"` and `#[inline(never)]` for the reason experiment 1 gives: the
/// optimiser cannot see into an entry point, cannot hoist anything out of the
/// caller's loop, and must treat caller-saved registers as clobbered. Without
/// this the comparison would flatter the engine by an amount nobody could name.
#[inline(never)]
extern "C" fn across_the_boundary(
    words: &mut [u64],
    at: usize,
    indexes: &FxMap<u32, FxMap<u32, u32>>,
    shape_of_type: &[u32],
    known: &[(u32, u32)],
    prototypes: &mut Aside<u64>,
    cell: usize,
    i: u64,
) -> u64 {
    let mut acc = own_property(words, at, indexes, shape_of_type, 3);
    acc = acc.wrapping_add(typed_as(known, 7));
    let link = words[at + 1];
    acc = acc.wrapping_add(link & 1);
    words[at] = header_word(7, WIDE_SLOTS as u32);
    for slot in 0..WIDE_SLOTS {
        words[at + 1 + slot] = 0;
    }
    prototypes[cell] = Some(i);
    words[at + 1] = i;
    acc.wrapping_add(words[at + 1])
}
