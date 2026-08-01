//! Boolean coercion and integer arithmetic — the two remaining primordials that
//! cost something on the emitted path.
//!
//! **Boolean.** `Repr::Tagged` in a condition lowers to
//! `call __rtsadp_to_boolean` UNCONDITIONALLY (`call.rs:1807-1812`). There is no
//! inline tag test in front of it, so `if (x)` on any un-proven value is a real
//! call. `genops::to_boolean` (`genops.rs:281`) is pure bit inspection for every
//! kind EXCEPT string, where it resolves the handle and calls `STRING_LEN` —
//! two more locks.
//!
//! **Integer.** `number_result` re-tightens an exact integral `f64` back into a
//! tagged int32 (`genops.rs:290-303`), so integer arithmetic on the Tagged path
//! is: unbox int32 → widen to `f64` → operate in `f64` → check exactness →
//! re-narrow to int32 → rebox. The native `iadd` never happens.

use crate::poly;

/// `__rtsadp_to_boolean` for the non-string kinds (the probe never produces a
/// string here; the string arm would add two more locks on top).
#[inline(never)]
pub extern "C" fn probe_to_boolean(v: i64) -> i64 {
    let w = v as u64;
    if !poly::is_boxed(w) {
        let f = poly::as_f64(w);
        return i64::from(f != 0.0 && !f.is_nan());
    }
    let tag = (w >> poly::TAG_SHIFT) & 0x7;
    if tag == poly::TAG_INT32 {
        return i64::from((w & 0xFFFF_FFFF) as u32 as i32 != 0);
    }
    // SINGLETON: only `true` is truthy among undefined/null/false/true.
    i64::from((w & poly::PAYLOAD_MASK) == 3)
}

/// `__rtsadp_add` on two tagged int32s — through `f64`, then re-tightened.
#[inline(never)]
pub extern "C" fn probe_adp_iadd(a: i64, b: i64) -> i64 {
    let x = poly::to_number(a as u64);
    let y = poly::to_number(b as u64);
    poly::number_result(x + y) as i64
}

// --- architecture kernel trampolines ---------------------------------------

/// `__rtsadp_err_pending` — reads the thread-local error slot. The real one is
/// an extern call in another crate, so it is an optimization barrier; this
/// replica is `#[inline(never)]` for the same reason.
#[inline(never)]
pub extern "C" fn probe_err_pending(_ignored: i64) -> i64 {
    use std::cell::Cell;
    thread_local! { static SLOT: Cell<i64> = const { Cell::new(0) }; }
    SLOT.with(|c| c.get())
}

/// The uniform 5-slot thunk ABI: every argument a tagged i64 word, fixed arity
/// regardless of the real one, tagged result.
#[inline(never)]
pub extern "C" fn probe_call_uniform(_env: i64, a: i64, b: i64, _c: i64, _d: i64) -> i64 {
    let x = crate::poly::to_number(a as u64);
    let y = crate::poly::to_number(b as u64);
    crate::poly::from_f64(x * y) as i64
}

/// The native signature a compiled language emits when the types are known.
#[inline(never)]
pub extern "C" fn probe_call_native_f64(a: f64, b: f64) -> f64 {
    a * b
}

/// `__rtsn_vec_push_by_payload` — one shard lock, push onto the object's `Vec`.
#[inline(never)]
pub extern "C" fn probe_vec_push_locked(payload: i64, value: i64) -> i64 {
    crate::slab::sharded::with_mut(payload as u64, |e| {
        if let Some(crate::slab::Entry::Vec(v)) = e {
            v.push(value);
        }
    });
    0
}

/// `__rtsadp_obj_set_proto` — the per-construction prototype wiring the engine
/// emits (pre-hoist). One locked touch of the object.
#[inline(never)]
pub extern "C" fn probe_obj_set_proto(payload: i64, proto: i64) -> i64 {
    crate::slab::sharded::with_mut(payload as u64, |e| {
        if let Some(crate::slab::Entry::Vec(v)) = e
            && let Some(slot) = v.first_mut()
        {
            *slot = proto;
        }
    });
    0
}

/// `alloc_entry`'s GC tick: every `GC_TICK_INTERVAL = 256` allocations the
/// engine calls `finish_cycle()` — a full conservative mark + sweep over every
/// shard. The probe has no collector, which is the leading candidate for why it
/// under-models the engine; this replicates the SWEEP half (walk every live
/// slot in every shard) at the real cadence.
#[inline(never)]
pub extern "C" fn probe_gc_tick(_ignored: i64) -> i64 {
    use std::cell::Cell;
    thread_local! { static TICK: Cell<u32> = const { Cell::new(0) }; }
    let fire = TICK.with(|c| {
        let v = c.get().wrapping_add(1);
        c.set(v);
        v % 256 == 0
    });
    if !fire {
        return 0;
    }
    // The engine does NOT sweep on every tick: `finish_cycle` is gated on
    // `GC_LIVE_FLOOR = 500_000` live handles OR `GC_LIVE_BYTES_FLOOR = 64 MB`
    // (`handles.rs:1429,1456`). A replica without the floor sweeps ~3900 times
    // over a slab that only grows, which overshot the engine by 14x. With the
    // floor AND actual reclamation it brackets the real cost instead.
    crate::slab::sharded::sweep_if_past_floor(500_000)
}
