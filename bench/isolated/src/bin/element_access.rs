//! **Experiment 6 — `arr[i]` as a call against `arr[i]` as instructions.**
//!
//! # The question
//!
//! `array index read` costs 16.63 ns against bun's 0.71. A property read on the
//! same table costs 4.97, and it is fast for exactly one reason: it does not
//! call. `crates/rts-cranelift` gives the language a `CachedGetIndirect`
//! terminator, so `obj.a` compiles to a header compare, a branch and a load,
//! with the runtime reached only on a miss.
//!
//! There is no equivalent for an element. `rts ir` on `a += arr[i & 1023]` emits
//! `Call { callee: __rts_get_indexed, args: [arr, <boxed index>] }` on every
//! iteration, and that entry point —
//! `crates/rts-core/src/entry/computed/access.rs:20` — performs, in order:
//!
//! 1. `opened(object, key)` — is the receiver a proxy
//! 2. `with_current` — reach the context
//! 3. `Value(object).as_slot()` — is the receiver a reference
//! 4. `array::as_index(context, key)` — unbox the key and decide whether the
//!    double is a canonical array index, which `array.rs:196` documents as the
//!    difference between an element and an ordinary property
//! 5. `context.elements_at(slot)` — an `Aside` lookup (a `Vec<Option<Slot>>`
//!    indexed by cell) and then a slab lookup, to reach a `Vec<u64>`
//! 6. `elements.get(at)` — a bounds check
//! 7. `array::visible(...)` — is this position a hole
//! 8. a `Found` enum, matched by the caller, which may mean calling a getter
//!
//! Every step is small. There are eight of them, and they sit behind a call the
//! optimiser cannot see into. **What would the same read cost as instructions?**
//!
//! # What is being compared
//!
//! Five shapes of "read element `i` of an array and add it to an accumulator":
//!
//! 1. **the entry point** — all eight steps, behind `#[inline(never)] extern "C"`,
//!    with the index arriving NaN-boxed as it does today.
//! 2. **the same, index arriving as a machine integer** — isolates what boxing
//!    the key costs from what the lookup costs.
//! 3. **the same steps, inlined** — no call, everything else identical. This is
//!    the ceiling of what removing *only* the call buys, and it is here because
//!    "the call is the cost" is the hypothesis a reader will have.
//! 4. **an inline fast path** — guard the header, load a length, compare, load a
//!    base, scaled load, check for a hole. Six instructions. This is what a
//!    `CachedElementGet` terminator would emit, and it needs the elements'
//!    base and length to be **reachable from the cell**, which today they are
//!    not: they live in a `Vec` behind two indirections in Rust memory.
//! 5. **a raw scaled load** — the floor, and what bun's 0.71 is close to.
//!
//! # The cost row 4 is hiding
//!
//! Row 4 assumes something the engine does not have: that a compiled load can
//! reach the elements. Making that true means the array's storage stops being a
//! `Vec<u64>` in a slab and becomes something addressed as `base + index × 8`
//! from a pointer the cell holds — which is a change to how arrays grow, how the
//! collector finds them, and what `push` does. That is a large piece of work in
//! `rts-cranelift` and `rts-core` both, and the whole reason to measure first is
//! that row 4 has to beat row 3 by enough to justify it. **If removing the call
//! alone (row 3) gets most of the way, the storage never has to change.**

use rts_isolated::{measure, opaque, report};

const BOX_BASE: u64 = 0xFFF8_0000_0000_0000;
const CELL_WORDS: usize = 16;

/// The type number an array cell carries, so the guard has something to compare.
const ARRAY_TYPE: u32 = 11;

/// A slab position, as `Slot` is.
#[derive(Clone, Copy)]
struct Slot(u32);

/// `Aside<Slot>`: a `Vec<Option<T>>` indexed by cell, from
/// `crates/rts-core/src/heap/aside.rs`.
struct Aside {
    entries: Vec<Option<Slot>>,
}

impl Aside {
    #[inline(always)]
    fn copied(&self, cell: u32) -> Option<Slot> {
        *self.entries.get(cell as usize)?
    }
}

/// The world an entry point reaches through `with_current`.
struct Context {
    region: Vec<u64>,
    array_elements: Aside,
    arrays: Vec<Vec<u64>>,
    undefined: u64,
}

thread_local! {
    static CURRENT: std::cell::RefCell<Vec<Context>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[inline(always)]
fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CURRENT.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let Some(context) = borrowed.last_mut() else {
            std::process::abort();
        };
        body(context)
    })
}

/// `Value::as_slot` — a reference's payload, or nothing.
#[inline(always)]
fn as_slot(word: u64) -> Option<u32> {
    if word & BOX_BASE == BOX_BASE {
        Some((word & 0xFFFF_FFFF) as u32)
    } else {
        None
    }
}

#[inline(always)]
fn from_slot(cell: u32) -> u64 {
    BOX_BASE | (2u64 << 48) | cell as u64
}

/// `array::as_index` — a canonical non-negative integer below 2^32-1, and
/// nothing else. `a[1.5]` and `a[-1]` are properties, not elements.
#[inline(always)]
fn as_index(key: u64) -> Option<usize> {
    if key & BOX_BASE == BOX_BASE {
        return None;
    }
    let number = f64::from_bits(key);
    if number < 0.0 || number.trunc() != number || number >= 4294967295.0 {
        return None;
    }
    Some(number as usize)
}

/// `opened` — is the receiver a proxy. Modelled as the type-number check it is.
#[inline(always)]
fn opened(context: &Context, cell: u32) -> bool {
    (context.region[cell as usize * CELL_WORDS] >> 32) as u32 == 99
}

/// `array::visible` — a hole reads as `undefined`.
#[inline(always)]
fn visible(context: &Context, held: u64) -> u64 {
    if held == u64::MAX { context.undefined } else { held }
}

/// The eight steps, exactly as `get_indexed` performs them, behind the call
/// boundary an entry point has.
#[inline(never)]
extern "C" fn get_indexed(object: u64, key: u64) -> u64 {
    with_current(|context| {
        let Some(cell) = as_slot(object) else {
            return context.undefined;
        };
        if opened(context, cell) {
            return context.undefined;
        }
        let Some(at) = as_index(key) else {
            return context.undefined;
        };
        let Some(store) = context.array_elements.copied(cell) else {
            return context.undefined;
        };
        let Some(elements) = context.arrays.get(store.0 as usize) else {
            return context.undefined;
        };
        match elements.get(at) {
            Some(held) => visible(context, *held),
            None => context.undefined,
        }
    })
}

/// The same, with the index already a machine integer.
#[inline(never)]
extern "C" fn get_indexed_machine(object: u64, at: i64) -> u64 {
    with_current(|context| {
        let Some(cell) = as_slot(object) else {
            return context.undefined;
        };
        if opened(context, cell) {
            return context.undefined;
        }
        let Some(store) = context.array_elements.copied(cell) else {
            return context.undefined;
        };
        let Some(elements) = context.arrays.get(store.0 as usize) else {
            return context.undefined;
        };
        match elements.get(at as usize) {
            Some(held) => visible(context, *held),
            None => context.undefined,
        }
    })
}

fn main() {
    let cells = 64;
    let mut region = vec![0u64; cells * CELL_WORDS];
    // Cell 3 is the array. Its header carries the array type number, and — for
    // row 4 only — slots 0 and 1 carry what an addressable design would put
    // there: the element base as a raw pointer, and the length.
    let array_cell: u32 = 3;
    region[array_cell as usize * CELL_WORDS] = (ARRAY_TYPE as u64) << 32;

    let elements: Vec<u64> = (0..1024u64).map(|i| (i as f64).to_bits()).collect();
    let base = elements.as_ptr();
    let length = elements.len();
    region[array_cell as usize * CELL_WORDS + 1] = base as u64;
    region[array_cell as usize * CELL_WORDS + 2] = length as u64;

    let mut entries = vec![None; cells];
    entries[array_cell as usize] = Some(Slot(0));

    let undefined = BOX_BASE | (1u64 << 48);
    CURRENT.with(|stack| {
        stack.borrow_mut().push(Context {
            region: region.clone(),
            array_elements: Aside { entries },
            arrays: vec![elements.clone()],
            undefined,
        })
    });

    let object = from_slot(array_cell);

    // Row 3's inlined copy needs its own world, since it does not go through the
    // thread-local. Same data, reached directly.
    let aside_direct: Vec<Option<Slot>> = {
        let mut v = vec![None; cells];
        v[array_cell as usize] = Some(Slot(0));
        v
    };
    let arrays_direct = vec![elements.clone()];
    let region_direct = region.clone();

    let rows = vec![
        // ------------------------------------------------------------ 1
        measure("1. entry point, index NaN-boxed (engine)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let key = ((i & 1023) as f64).to_bits();
                acc = acc.wrapping_add(get_indexed(opaque(object), key));
            }
            acc
        }),
        // ------------------------------------------------------------ 2
        measure("2. entry point, index a machine integer", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(get_indexed_machine(opaque(object), (i & 1023) as i64));
            }
            acc
        }),
        // ------------------------------------------------------------ 3
        measure("3. the same eight steps, no call", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let word = opaque(object);
                let at = (i & 1023) as usize;
                let value = (|| {
                    let cell = as_slot(word)?;
                    if (region_direct[cell as usize * CELL_WORDS] >> 32) as u32 == 99 {
                        return None;
                    }
                    let store = (*aside_direct.get(cell as usize)?)?;
                    let elements = arrays_direct.get(store.0 as usize)?;
                    let held = *elements.get(at)?;
                    Some(if held == u64::MAX { undefined } else { held })
                })()
                .unwrap_or(undefined);
                acc = acc.wrapping_add(value);
            }
            acc
        }),
        // ------------------------------------------------------------ 4
        measure("4. inline fast path: guard, bound, load", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                let word = opaque(object);
                let at = (i & 1023) as usize;
                // What a `CachedElementGet` terminator would emit. Six steps:
                // is it a reference, does the header match the array type, is
                // the index below the length, load the base, scaled load, is it
                // a hole.
                let cell = (word & 0xFFFF_FFFF) as usize;
                let header = region_direct[cell * CELL_WORDS];
                let value = if (header >> 32) as u32 != ARRAY_TYPE {
                    undefined
                } else {
                    let len = region_direct[cell * CELL_WORDS + 2] as usize;
                    if at >= len {
                        undefined
                    } else {
                        let base = region_direct[cell * CELL_WORDS + 1] as *const u64;
                        // SAFETY: `base` points at `elements`, which outlives
                        // this loop, and `at < len` was just checked.
                        let held = unsafe { *base.add(at) };
                        if held == u64::MAX { undefined } else { held }
                    }
                };
                acc = acc.wrapping_add(value);
            }
            acc
        }),
        // ------------------------------------------------------------ 5
        measure("5. a raw scaled load (the floor)", |n| {
            let mut acc = 0u64;
            for i in 0..n {
                acc = acc.wrapping_add(elements[opaque(i & 1023) as usize]);
            }
            acc
        }),
    ];

    report("Experiment 6 - reading an element", &rows);
    println!();
    println!("Row 3 removes ONLY the call. Row 4 also removes the eight steps, and");
    println!("needs the elements to be reachable from the cell — storage work in two");
    println!("crates. If row 3 is already close to row 4, that work is not justified.");
}
