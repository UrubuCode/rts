//! **Experiment 3 — what `new C()` costs, and what a precomputed layout would.**
//!
//! # The question
//!
//! `bench/analytic.ts` measures `new Callee()` — a class with one field
//! initialiser — at **90.89 ns**, against 0.53 on bun and 0.38 on node. An
//! object literal with two fields costs 1.22 ns in the same table, because
//! `crates/rts-codegen/src/emit/escape.rs` removes the allocation entirely. So
//! the number is not "allocation is slow here". Something about *classes* costs
//! ninety nanoseconds.
//!
//! The engine's object model is a shape tree, `crates/rts-cranelift/src/shape/tree.rs`.
//! A layout is a chain of one-property nodes, two objects built the same way
//! arrive at the same node, and the step from one layout to the next is a hash
//! lookup:
//!
//! ```ignore
//! transitions: HashMap<(Option<ShapeId>, Key, Repr), ShapeId>
//! ```
//!
//! That lookup happens **at run time, per property, per object**. Building an
//! object with three fields is three hash lookups, every time one is built, for
//! a layout the compiler could have named before the program started.
//!
//! # What is being compared
//!
//! Four shapes of "make an object with N fields and read one back":
//!
//! 1. **transition per field** — the engine's model. A hash lookup keyed by
//!    `(parent, key, repr)` for each field, then the stores.
//! 2. **transition per field, but the map is warm and small** — the same, with
//!    the map holding only this chain. Isolates "hashing" from "hashing in a map
//!    that has grown", because a real program's transition map holds every
//!    layout it has ever built.
//! 3. **precomputed layout** — the shape is known before the loop; the object is
//!    a header store plus N field stores. This is what a compiler that resolved
//!    the layout at compile time would emit.
//! 4. **the stores alone** — no header, no layout, just the memory traffic. The
//!    floor: no object model can cost less than writing the fields.
//!
//! Every shape allocates out of the same bump region and writes the same bytes,
//! so the difference between rows is the *layout bookkeeping* and nothing else.
//!
//! # RESULT, and the reason it does not license what it looks like it licenses
//!
//! Release, 2026-08-21:
//!
//! | | ns/op |
//! |---|---:|
//! | 1. transition per field, crowded map | **20.52** |
//! | 2. transition per field, empty map | 21.15 |
//! | 3. precomputed layout, header + stores | **3.10** |
//! | 4. the stores alone | 3.95 |
//!
//! So resolving a three-field layout by transition costs about **17 ns more**
//! than storing into one already resolved, and — rows 1 against 2 — **how many
//! layouts the map holds does not matter**, which kills the "the transition map
//! has grown" theory before anyone spends a day on it.
//!
//! **But `new Callee()` does not take this path, and that is the important
//! finding.** Shape transitions in the engine are memoised twice over:
//! `ShapeTree::transitions` globally, and each access site remembers a
//! (before-header → offset, after-header) triple. `RTS_CACHE_CENSUS=1` over
//! 200 000 iterations of `new Callee()` reports **0 misses and 0 sites**, and
//! `RTS_CACHE_WHY=1` over an escaping-literal loop prints three lines — one per
//! key, for the life of the program, not per object. **None of the 90.9 ns is
//! transition lookup.**
//!
//! What this experiment therefore prices is the **by-name** write path,
//! `objects::put` — which a running program reaches on a cache miss and which
//! *installation* reaches 1 497 times before a program starts. That is a real
//! and useful number. It is not a number about `new`.
//!
//! Where the 90.9 ns actually is: `allocate_for_target` runs a full
//! prototype-chain property read on every `new` — a `well_known("prototype")`
//! scan, a chain walk of ~6 hash probes, and a linear `typed_as` find. See
//! `docs/codegen/plan.md` §3 and L3.
//!
//! # What this deliberately does not model
//!
//! The constructor call, the prototype link, and `this` escaping into the
//! constructor body. Those are real parts of the 90.89 ns and they belong to the
//! language layer; this experiment is about the runtime's half. A row here is a
//! lower bound on the saving, not the saving.

use rts_isolated::{measure, opaque, report};
use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// The hasher `crates/rts-cranelift/src/shape/tree.rs` uses, transcribed.
///
/// That file imports `rustc_hash::FxHashMap` and says why: the keys are numbers
/// the crate mints itself, so there is no untrusted input and SipHash buys
/// nothing. Using `std`'s default here instead would measure SipHash and report
/// a cost the engine does not pay — so the algorithm is transcribed rather than
/// substituted. It is FxHasher: multiply-and-rotate, one round per word.
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

/// A shape's number, as `ShapeId` is.
type ShapeId = u32;

/// A property's number, as `Key` is.
type Key = u32;

/// A representation's number, standing in for `Repr`.
type Repr = u32;

/// The empty layout every object starts at, `ShapeTree::root`.
const ROOT: ShapeId = u32::MAX;

/// How many slots a cell holds — `crates/rts-core/src/heap/region/mod.rs:179`.
const INLINE_SLOTS: usize = 15;

/// A cell: one header word and fifteen field words, 128 bytes.
const CELL_WORDS: usize = 1 + INLINE_SLOTS;

/// A region: one contiguous allocation of fixed-stride cells, addressed as
/// `base + index * stride`, which is what `heap::region` is.
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

    /// A bump allocation, and a wrap so a long run never leaves the region.
    ///
    /// Wrapping rather than growing on purpose: growth would put a `realloc`
    /// inside the thing being measured, and re-using cells keeps the working
    /// set in cache, which is the case an allocation benchmark should be
    /// measuring — a program that allocates in a loop reaches warm memory.
    #[inline(always)]
    fn alloc(&mut self, type_id: u32) -> usize {
        if self.next + CELL_WORDS > self.words.len() {
            self.next = 0;
        }
        let cell = self.next;
        self.next += CELL_WORDS;
        self.words[cell] = header_word(type_id, INLINE_SLOTS as u32);
        cell
    }

    #[inline(always)]
    fn set_field(&mut self, cell: usize, slot: usize, value: u64) {
        self.words[cell + 1 + slot] = value;
    }

    #[inline(always)]
    fn field(&self, cell: usize, slot: usize) -> u64 {
        self.words[cell + 1 + slot]
    }
}

/// The header: a type number beside the cell's width, as `header_word` is.
#[inline(always)]
fn header_word(type_id: u32, width: u32) -> u64 {
    ((type_id as u64) << 32) | width as u64
}

/// The transition map, as `ShapeTree` holds it.
struct Shapes {
    transitions: FxMap<(ShapeId, Key, Repr), ShapeId>,
    /// The type number each layout was registered as, which
    /// `ShapeTree::types` holds and `Context::layout_of` reads.
    types: FxMap<ShapeId, u32>,
    next: ShapeId,
}

impl Shapes {
    fn new() -> Self {
        Shapes {
            transitions: FxMap::default(),
            types: FxMap::default(),
            next: 0,
        }
    }

    /// One step of the chain: exactly the lookup the engine performs per
    /// property write.
    #[inline(always)]
    fn transition(&mut self, from: ShapeId, key: Key, repr: Repr) -> ShapeId {
        match self.transitions.get(&(from, key, repr)) {
            Some(found) => *found,
            None => {
                let made = self.next;
                self.next += 1;
                self.transitions.insert((from, key, repr), made);
                made
            }
        }
    }

    /// The registered aggregate for a layout, which is what goes in the header
    /// and what a cached read guards against.
    #[inline(always)]
    fn layout_of(&mut self, shape: ShapeId) -> u32 {
        match self.types.get(&shape) {
            Some(found) => *found,
            None => {
                let made = shape;
                self.types.insert(shape, made);
                made
            }
        }
    }
}

/// Fills a shape tree with unrelated layouts, so the transition map is the size
/// a real program's is rather than the size this experiment's chain needs.
///
/// A hash map with three entries lives in one cache line and answers in a few
/// cycles; a hash map with thousands does not. `bench/analytic.ts` alone builds
/// a shape for every object literal, every class, every closure environment and
/// every module scope in the file. Measuring against an empty map would
/// understate the engine's cost, which is the opposite of the error this tree's
/// rules are written against.
fn crowd(shapes: &mut Shapes, layouts: u32) {
    for layout in 0..layouts {
        let mut at = ROOT;
        for property in 0..4u32 {
            at = shapes.transition(at, layout * 8 + property, 1);
        }
        shapes.layout_of(at);
    }
}

/// How many fields the object under test has. Three, because a class with a
/// couple of initialised fields is the ordinary case and `Callee` in
/// `analytic.ts` has one — three makes the per-field cost visible without
/// pretending objects are wide.
const FIELDS: usize = 3;

fn main() {
    let mut region = Region::new(4096);
    let mut crowded = Shapes::new();
    crowd(&mut crowded, 2000);
    let mut sparse = Shapes::new();

    // The layout a compiler would have resolved: walked once, out here.
    let precomputed_type = {
        let mut at = ROOT;
        for field in 0..FIELDS as u32 {
            at = crowded.transition(at, 100 + field, 1);
        }
        crowded.layout_of(at)
    };

    let rows = vec![
        // ------------------------------------------------------------ 1
        measure("1. transition per field, crowded map (engine)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let mut shape = ROOT;
                let cell = region.alloc(0);
                for field in 0..FIELDS as u32 {
                    shape = crowded.transition(shape, 100 + field, 1);
                    region.set_field(cell, field as usize, opaque(i));
                }
                let ty = crowded.layout_of(shape);
                region.words[cell] = header_word(ty, INLINE_SLOTS as u32);
                acc = acc.wrapping_add(region.field(cell, 0));
            }
            acc
        }),
        // ------------------------------------------------------------ 2
        measure("2. transition per field, empty map", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let mut shape = ROOT;
                let cell = region.alloc(0);
                for field in 0..FIELDS as u32 {
                    shape = sparse.transition(shape, 100 + field, 1);
                    region.set_field(cell, field as usize, opaque(i));
                }
                let ty = sparse.layout_of(shape);
                region.words[cell] = header_word(ty, INLINE_SLOTS as u32);
                acc = acc.wrapping_add(region.field(cell, 0));
            }
            acc
        }),
        // ------------------------------------------------------------ 3
        measure("3. precomputed layout, header + stores", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let cell = region.alloc(precomputed_type);
                for field in 0..FIELDS {
                    region.set_field(cell, field, opaque(i));
                }
                acc = acc.wrapping_add(region.field(cell, 0));
            }
            acc
        }),
        // ------------------------------------------------------------ 4
        measure("4. the stores alone (the floor)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let cell = region.alloc(opaque(precomputed_type));
                for field in 0..FIELDS {
                    region.set_field(cell, field, opaque(i));
                }
                acc = acc.wrapping_add(region.field(cell, 0));
            }
            acc
        }),
    ];

    report("Experiment 3 - making an object", &rows);
    println!();
    println!("Rows 1 and 2 differ only in how many layouts the transition map holds,");
    println!("so their difference is what hashing into a real program's map costs");
    println!("over hashing into an empty one. Row 3 is what a compile-time layout");
    println!("would emit; row 1 minus row 3 is what resolving it at run time costs.");
}
