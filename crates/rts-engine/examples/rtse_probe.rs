//! `rtse_probe` — what actually costs, on the object read/write path.
//!
//! Run: `cargo run --release --example rtse_probe -p rts-engine`
//! (release only — a debug number here is not a number).
//!
//! ## Why this exists
//!
//! A previous investigation into rewriting the object layout was refuted, and it
//! left exactly one gap open: `RTS_REPR_STATS` counts call-site FREQUENCY, not
//! COST. Nobody knows whether the per-access `Mutex` or the `Box<Vec<i64>>`
//! indirection dominates — and a storage decision taken without that number is a
//! guess wearing a measurement's clothes.
//!
//! ## The design, and how it can FALSIFY
//!
//! Two independent variables, so the arms form a 2x2 and each variable is
//! isolated by a single comparison rather than by argument:
//!
//! |            | boxed `Vec<i64>`  | flat inline slots |
//! |------------|-------------------|-------------------|
//! | **mutex**  | A (today)         | C                 |
//! | **no lock**| B                 | D                 |
//!
//! - `A -> B` isolates the LOCK (same layout, lock removed).
//! - `A -> C` isolates the LAYOUT (same lock, indirection removed).
//! - `D` is the ceiling both changes together could reach.
//! - `NATIVE` is a plain Rust struct — the 2.01 ms reference from
//!   `docs/specs/FUTURE_OPTIMIZATION.md`, i.e. what "1x" means.
//!
//! The falsification condition is stated up front, before the numbers exist:
//! if `A -> B` is flat, the lock is NOT the problem and no amount of lock-free
//! engineering will pay; if `A -> C` is flat, the layout is not the problem and
//! flattening `Entry::Vec` is wasted work. Either outcome kills a plan, which is
//! the point of running it.
//!
//! ## What this probe does NOT measure
//!
//! It models the storage path only. It does not include Cranelift's lost
//! optimizations across the opaque extern call (keeping `p.x` in a register
//! across `p.x * p.y`), nor NaN-box tag traffic, nor call overhead itself. Those
//! are real and are measured separately by the engine's own `RTS_REPR_STATS`.
//! So the arms below are a LOWER bound on what a storage change can buy — if the
//! spread here is small, the storage question is settled negatively and the cost
//! lives somewhere else entirely.

use std::hint::black_box;
use std::sync::Mutex;
use std::time::Instant;

use rts_engine::heap::handles::{Entry, alloc_entry, with_entry};

/// Matches `bench/objbench.ts`: 3M constructions of a 2-field object, each read
/// back twice (`p.x * p.y`).
const N: usize = 3_000_000;

/// Fields per object, matching `class P { x, y }`.
const K: usize = 2;

fn main() {
    println!("rtse_probe — object storage cost, {N} iterations of `new P(i, i+1); p.x * p.y`\n");

    let native = arm_native();
    let a = arm_a_real_heap();
    let b = arm_b_boxed_nolock();
    let c = arm_c_flat_mutex();
    let d = arm_d_flat_nolock();
    let b2 = arm_b2_boxed_nolock_noalloc();

    let rows = [
        ("NATIVE  plain Rust struct", native),
        ("A  mutex + Box<Vec> + alloc  (today)", a),
        ("B  NO LOCK + Box<Vec> + alloc", b),
        ("B2 NO LOCK + Box<Vec>, alloc HOISTED", b2),
        ("C  mutex + flat inline", c),
        ("D  NO LOCK + flat inline", d),
    ];
    println!("{:<38} {:>10} {:>10}", "arm", "ms", "x native");
    for (name, ms) in rows {
        println!("{name:<38} {ms:>10.2} {:>10.1}", ms / native);
    }

    println!("\n-- what each comparison isolates --");
    println!("lock                 (A -> B):  {:.2}x", a / b);
    println!("per-object alloc     (B -> B2): {:.2}x", b / b2);
    println!("pointer chase alone  (B2 -> D): {:.2}x", b2 / d);
    println!("layout, alloc+chase  (B -> D):  {:.2}x", b / d);
    println!("everything           (A -> D):  {:.2}x", a / d);

    println!(
        "\nCAVEAT, stated so the numbers are not over-read: arms C and D reuse ONE\n\
         slot, so they run in a hot cache line a real workload would not always get.\n\
         The honest isolation is B -> B2 (allocation) and B2 -> D (indirection);\n\
         `x native` is indicative only, since NATIVE pays black_box on every field\n\
         while D does not. This probe models STORAGE only — it excludes call\n\
         overhead and the Cranelift optimizations lost across an opaque extern\n\
         call, so it is a LOWER bound on what a storage change can buy."
    );
}

// ---------------------------------------------------------------- NATIVE

struct Point {
    x: i64,
    y: i64,
}

/// The reference. No heap indirection, no lock, fields in registers.
fn arm_native() -> f64 {
    let t = Instant::now();
    let mut s: i64 = 0;
    for i in 0..N as i64 {
        let p = black_box(Point { x: i, y: i + 1 });
        s = s.wrapping_add(black_box(p.x).wrapping_mul(black_box(p.y)));
    }
    black_box(s);
    ms(t)
}

// ---------------------------------------------------------------- ARM A

/// TODAY's path, using the REAL engine heap — not a model of it. `Entry::Vec`
/// holds `Box<Vec<i64>>`, so reaching a field is
/// handle -> shard -> slab -> Slot -> Entry -> Box -> Vec buffer -> element,
/// and every access takes the shard `Mutex`.
///
/// Using the real `alloc_entry`/`with_entry` matters: a hand-written model of
/// the slow path would be a strawman, and a strawman that loses proves nothing.
fn arm_a_real_heap() -> f64 {
    let t = Instant::now();
    let mut s: i64 = 0;
    for i in 0..N as i64 {
        let h = alloc_entry(Entry::Vec(Box::new(vec![i, i + 1])));
        let x = with_entry(h, |e| match e {
            Some(Entry::Vec(v)) => v[0],
            _ => 0,
        });
        let y = with_entry(h, |e| match e {
            Some(Entry::Vec(v)) => v[1],
            _ => 0,
        });
        s = s.wrapping_add(black_box(x).wrapping_mul(black_box(y)));
    }
    black_box(s);
    ms(t)
}

// ------------------------------------------------- shared slab machinery

/// A slot holding the SAME shape of payload as `Entry::Vec` — a boxed vector —
/// so arm B differs from arm A in the lock and nothing else.
struct BoxedSlot {
    generation: u16,
    marked: bool,
    fields: Option<Box<Vec<i64>>>,
}

/// A slot with the fields stored INLINE. `K` is fixed here because the probe
/// models a 2-field class; a real implementation needs size classes or an
/// overflow path, and that cost is NOT modelled here (noted so the number is not
/// read as more than it is).
struct FlatSlot {
    generation: u16,
    marked: bool,
    fields: [i64; K],
}

// ---------------------------------------------------------------- ARM B

/// Same boxed layout as today, lock removed. `A / B` is the lock's price.
fn arm_b_boxed_nolock() -> f64 {
    let mut slab: Vec<BoxedSlot> = Vec::with_capacity(1024);
    let t = Instant::now();
    let mut s: i64 = 0;
    for i in 0..N as i64 {
        let idx = alloc_boxed(&mut slab, i);
        let x = slab[idx].fields.as_ref().map_or(0, |f| f[0]);
        let y = slab[idx].fields.as_ref().map_or(0, |f| f[1]);
        s = s.wrapping_add(black_box(x).wrapping_mul(black_box(y)));
    }
    black_box(s);
    ms(t)
}

/// Boxed layout, no lock, and the per-object allocation HOISTED out of the loop:
/// the box is created once and its contents overwritten. `B -> B2` therefore
/// prices the malloc, and `B2 -> D` prices the pointer chase ALONE — which is
/// what "layout" actually means. Without this arm, `A -> C` conflates the two
/// and would credit flattening with a win that is really the allocator's.
fn arm_b2_boxed_nolock_noalloc() -> f64 {
    let mut slab: Vec<BoxedSlot> = Vec::with_capacity(1024);
    slab.push(BoxedSlot {
        generation: 0,
        marked: false,
        fields: Some(Box::new(vec![0; K])),
    });
    let t = Instant::now();
    let mut s: i64 = 0;
    for i in 0..N as i64 {
        slab[0].generation = slab[0].generation.wrapping_add(1);
        if let Some(f) = slab[0].fields.as_mut() {
            f[0] = i;
            f[1] = i + 1;
        }
        let x = slab[0].fields.as_ref().map_or(0, |f| f[0]);
        let y = slab[0].fields.as_ref().map_or(0, |f| f[1]);
        s = s.wrapping_add(black_box(x).wrapping_mul(black_box(y)));
    }
    black_box(s);
    ms(t)
}

fn alloc_boxed(slab: &mut Vec<BoxedSlot>, i: i64) -> usize {
    // A free list would reuse slots; the probe reuses slot 0 once warm so the
    // measurement is the ACCESS path, not allocator growth. Growing an unbounded
    // Vec would measure realloc, which is a different question.
    if slab.is_empty() {
        slab.push(BoxedSlot {
            generation: 0,
            marked: false,
            fields: None,
        });
    }
    slab[0].generation = slab[0].generation.wrapping_add(1);
    slab[0].fields = Some(Box::new(vec![i, i + 1]));
    0
}

// ---------------------------------------------------------------- ARM C

/// Fields inline, lock kept. `A / C` is the indirection's price.
fn arm_c_flat_mutex() -> f64 {
    let slab: Mutex<Vec<FlatSlot>> = Mutex::new(Vec::with_capacity(1024));
    {
        let mut g = slab.lock().unwrap();
        g.push(FlatSlot {
            generation: 0,
            marked: false,
            fields: [0; K],
        });
    }
    let t = Instant::now();
    let mut s: i64 = 0;
    for i in 0..N as i64 {
        {
            let mut g = slab.lock().unwrap();
            g[0].generation = g[0].generation.wrapping_add(1);
            g[0].fields = [i, i + 1];
        }
        let x = {
            let g = slab.lock().unwrap();
            g[0].fields[0]
        };
        let y = {
            let g = slab.lock().unwrap();
            g[0].fields[1]
        };
        s = s.wrapping_add(black_box(x).wrapping_mul(black_box(y)));
    }
    black_box(s);
    ms(t)
}

// ---------------------------------------------------------------- ARM D

/// Both changes. The ceiling a storage rewrite could reach — still short of
/// NATIVE, and the size of that remaining gap is the probe's real finding.
fn arm_d_flat_nolock() -> f64 {
    let mut slab: Vec<FlatSlot> = Vec::with_capacity(1024);
    slab.push(FlatSlot {
        generation: 0,
        marked: false,
        fields: [0; K],
    });
    let t = Instant::now();
    let mut s: i64 = 0;
    for i in 0..N as i64 {
        slab[0].generation = slab[0].generation.wrapping_add(1);
        slab[0].fields = [i, i + 1];
        let x = slab[0].fields[0];
        let y = slab[0].fields[1];
        s = s.wrapping_add(black_box(x).wrapping_mul(black_box(y)));
    }
    black_box(s);
    ms(t)
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}
