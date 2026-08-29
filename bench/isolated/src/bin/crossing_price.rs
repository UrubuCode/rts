//! Price ONE crossing's parts: direct call vs indirect call, the throw check,
//! reaching the context, and register pressure across the call.
//!
//! `entry_tax.rs` measured a DIRECT `call rel32`. The engine emits an INDIRECT
//! call: entry points are declared `Linkage::Import` (symbols/table.rs:55), so
//! `colocated` is false and cranelift materialises the address and calls a
//! register. This bin exists to price that difference and the ones around it.

use std::cell::{Cell, RefCell};
use std::hint::black_box;
use std::time::Instant;

#[repr(C)]
pub struct Context {
    counter: u64,
    _pad: [u64; 47],
}

thread_local! {
    static CONTEXTS: RefCell<Vec<Context>> = const { RefCell::new(Vec::new()) };
    static THROWN: Cell<u64> = const { Cell::new(0) };
}

fn with_current<T>(body: impl FnOnce(&mut Context) -> T) -> T {
    CONTEXTS.with(|stack| {
        let mut borrowed = stack.borrow_mut();
        let context = borrowed.last_mut().unwrap();
        body(context)
    })
}

// --- the callees ------------------------------------------------------------

#[inline(never)]
extern "C" fn nothing(v: u64) -> u64 {
    black_box(v)
}

#[inline(never)]
extern "C" fn ctx(v: u64) -> u64 {
    with_current(|c| {
        c.counter = c.counter.wrapping_add(1);
        v
    })
}

// A stand-in for `to_boolean`: reach the context, read two fields off it,
// and answer from the value's bits without touching the heap.
#[inline(never)]
extern "C" fn to_boolean_like(v: u64) -> u64 {
    with_current(|c| {
        let singletons = black_box(c.counter);
        let kinds = black_box(c._pad[0]);
        u64::from(v != singletons && v != kinds && v != 0)
    })
}

#[inline(never)]
extern "C" fn ctx_passed(v: u64, c: &mut Context) -> u64 {
    c.counter = c.counter.wrapping_add(1);
    v
}

const N: u64 = 200_000_000;

fn time(name: &str, f: impl Fn() -> u64) {
    let mut best = f64::MAX;
    for _ in 0..5 {
        let t = Instant::now();
        black_box(f());
        let ns = t.elapsed().as_nanos() as f64 / N as f64;
        if ns < best {
            best = ns;
        }
    }
    println!("{name:<52}{best:>8.3}");
}

fn main() {
    CONTEXTS.with(|s| {
        s.borrow_mut().push(Context {
            counter: 1,
            _pad: [2; 47],
        })
    });

    // Direct: the compiler sees the symbol; `call rel32`.
    time("1. direct call, body does nothing", || {
        let mut a = 0u64;
        for i in 0..N {
            a = a.wrapping_add(nothing(black_box(i)));
        }
        a
    });

    // Indirect: the address is opaque, so it is materialised and called.
    let p_nothing: extern "C" fn(u64) -> u64 = black_box(nothing);
    time("2. INDIRECT call, body does nothing", || {
        let mut a = 0u64;
        for i in 0..N {
            a = a.wrapping_add(p_nothing(black_box(i)));
        }
        a
    });

    // Indirect + the throw check compiled code emits after it.
    let thrown_addr = THROWN.with(|t| t.as_ptr());
    time("3. indirect + throw check (load/cmp/branch)", || {
        let mut a = 0u64;
        for i in 0..N {
            let r = p_nothing(black_box(i));
            if unsafe { *thrown_addr } != 0 {
                return 0;
            }
            a = a.wrapping_add(r);
        }
        a
    });

    // entry_tax.rs shape 4: the same field read-modify-write with the context
    // PASSED IN. Subtracting this from shape 4 isolates REACHING the context
    // from DOING the work.
    let mut standalone = Context { counter: 1, _pad: [2; 47] };
    let sp: *mut Context = &raw mut standalone;
    let p_passed: extern "C" fn(u64, &mut Context) -> u64 = black_box(ctx_passed);
    time("4a. indirect + &mut Context PASSED IN (the floor)", || {
        let mut a = 0u64;
        for i in 0..N {
            a = a.wrapping_add(p_passed(black_box(i), unsafe { &mut *sp }));
        }
        a
    });

    // Indirect + reaching the context the way the engine does.
    let p_ctx: extern "C" fn(u64) -> u64 = black_box(ctx);
    time("4. indirect + with_current (Vec index)", || {
        let mut a = 0u64;
        for i in 0..N {
            a = a.wrapping_add(p_ctx(black_box(i)));
        }
        a
    });

    // The whole shape: indirect + with_current + a to_boolean-sized body
    // + the throw check.
    let p_tb: extern "C" fn(u64) -> u64 = black_box(to_boolean_like);
    time("5. FULL: indirect + with_current + body + check", || {
        let mut a = 0u64;
        for i in 0..N {
            let r = p_tb(black_box(i));
            if unsafe { *thrown_addr } != 0 {
                return 0;
            }
            a = a.wrapping_add(r);
        }
        a
    });

    // The same, with six values live across the call — what an engine frame
    // holding Tagged locals forces the register allocator to spill.
    time("6. FULL + 6 values live across the call", || {
        let (mut a, mut b, mut c, mut d, mut e, mut f) = (0u64, 1u64, 2u64, 3u64, 4u64, 5u64);
        for i in 0..N {
            let r = p_tb(black_box(i));
            if unsafe { *thrown_addr } != 0 {
                return 0;
            }
            a = a.wrapping_add(r);
            b = b.wrapping_add(a ^ r);
            c = c.wrapping_add(b);
            d = d.wrapping_add(c ^ r);
            e = e.wrapping_add(d);
            f = f.wrapping_add(e ^ r);
        }
        a ^ b ^ c ^ d ^ e ^ f
    });

    // The zero-crossing control: the same loop with no call at all.
    time("0. no call at all (the loop)", || {
        let mut a = 0u64;
        for i in 0..N {
            a = a.wrapping_add(black_box(i));
        }
        a
    });
}
