//! `rts:rigid` — the four floats-in, floats-out members a program calls.
//!
//! # Why there is no handle, no `create` and no state kept across calls
//!
//! The first design had `rigid.create(pos, vel, ext)` answering a handle, the
//! way the GPU backend's `rbInit` does. It was dropped for a reason that is
//! specific to this engine: a handle would have to remember the three typed
//! arrays as VALUES between calls, and the collector cannot see a value held
//! outside the heap unless it is told to — `entry::external` is the mechanism and
//! it exists for N-API. Holding three views in a table nothing traces is a
//! program whose buffers are swept while the solver still points at them.
//!
//! So the call carries everything: the three body buffers and the world record,
//! which is four arguments — the most a native takes. The sub-step count went
//! into `world[3]`, which the kernel's own layout leaves unused and comments as
//! `-`. Nothing is remembered, so nothing can be stale, and the only thing that
//! survives a call is the scratch in [`Solver`], which holds no body state.
//!
//! # The borrow is dropped before the solver runs, and that is not optional
//!
//! `with_current` holds a `RefCell` borrow for the length of its body, and a
//! panic in an `extern "C"` frame cannot unwind — it aborts the process. So the
//! pointers are taken inside a short borrow and every slice is built and used
//! outside it, which is the same shape `rts-napi-rwk`'s `buffers.rs` uses over
//! the same entry point.
//!
//! # No thread this crate starts ever touches the engine
//!
//! `rayon`'s workers see `&[f32]` and `&mut [f32]` and nothing else: no
//! `Context`, no cell, no user code, no allocation in the engine's heap. That is
//! the architecture rule this crate was written against — a `Context` is reached
//! through a thread-local, so a worker that touched one would be looking at a
//! different thread's runtime — and it is the reason the surface is shaped as
//! buffers rather than as objects with methods.

use rts_core::entry::{self, Context, Provided};

use crate::solver::Solver;

/// Registers the module. Called by the host, like every other surface.
pub fn namespace(context: &mut Context) -> u64 {
    entry::make_namespace(context, RIGID)
}

const RIGID: &[(&str, Provided)] = &[("step", step), ("threads", threads)];

thread_local! {
    /// The scratch the solver reuses. Per thread because a `Context` is, and
    /// this is only ever reached from the thread running JavaScript — the
    /// workers below it never see it.
    ///
    /// It holds no body state: a grid and two snapshot vectors, all overwritten
    /// at the top of every sub-step. So a program stepping two different scenes
    /// through one solver gets the same answer as stepping them through two.
    static SOLVER: std::cell::RefCell<Solver> = std::cell::RefCell::new(Solver::new());
}

/// `rigid.step(pos, vel, ext, world)` — advances the scene by `world[3]`
/// sub-steps, in place, and answers how many bodies it moved.
///
/// Zero is the refusal, and every refusal is a fact about the arguments: one of
/// them is not a typed array, two of them are the same buffer, or a length is not
/// a whole number of four-float records. Refusing rather than approximating is
/// the same choice the rest of this engine's surface makes — a solver reading
/// three floats of one body and one of the next produces a scene that runs and is
/// wrong, which is worse than a call that answers zero.
extern "C" fn step(_e: u64, _this: u64, pos: u64, vel: u64, ext: u64, world: u64) -> u64 {
    let Some(views) = entry::with_runtime(|context| windows(context, [pos, vel, ext, world]))
    else {
        return entry::make_number(0.0);
    };
    // Outside the borrow from here down. Nothing below reaches the engine.
    let [pos, vel, ext, world] = views;
    // SAFETY: each pointer is a distinct typed array's own `Vec<u8>` — checked
    // disjoint in `windows` — the borrow that produced them has been dropped, and
    // nothing else runs on this thread until this call returns, because the
    // solver calls no user code. The buffers' addresses do not move while another
    // is allocated, and nothing here allocates in the engine at all.
    let (pos, vel) = unsafe { (floats_mut(pos), floats_mut(vel)) };
    let (ext, world) = unsafe { (floats(ext), floats(world)) };

    let substeps = match world.len() >= 4 && world[3].is_finite() && world[3] >= 1.0 {
        true => world[3] as usize,
        // Absent is one step, not none: the field is `-` in the layout this
        // shares with the GPU kernel, so a buffer written for that backend and
        // handed here still advances rather than silently standing still.
        false => 1,
    };
    SOLVER.with(|solver| solver.borrow_mut().step(pos, vel, ext, world, substeps));
    entry::make_number((pos.len() / 4) as f64)
}

/// `rigid.threads()` — how many workers a step is spread over.
///
/// Present because a performance number that does not say how many threads
/// produced it is not a number, and the program measuring one should not have to
/// guess what `rayon` decided.
extern "C" fn threads(_e: u64, _this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    entry::make_number(rayon::current_num_threads() as f64)
}

/// Where each argument's bytes are, once every one of them has been checked.
///
/// The checks are together rather than at each use because they are one
/// question — "are these four arguments four distinct, well-formed float
/// windows?" — and a caller that answered it in pieces would have three places to
/// forget the disjointness half.
fn windows(context: &mut Context, values: [u64; 4]) -> Option<[(*mut u8, usize); 4]> {
    let mut found = [(std::ptr::null_mut(), 0usize); 4];
    for (at, value) in values.into_iter().enumerate() {
        let (bytes, length) = entry::bytes_pointer(context, value)?;
        // A window that is not a whole number of `f32`s, or not aligned for one.
        // The language guarantees the alignment for a `Float32Array`, so this
        // catches a `DataView` or a `Uint8Array` handed in by mistake — which
        // would otherwise be an unaligned read, undefined behaviour rather than a
        // wrong answer.
        if length % 4 != 0 || !bytes.cast::<f32>().is_aligned() {
            return None;
        }
        found[at] = (bytes, length);
    }
    // Disjoint, because two of them become `&mut [f32]` at once. Two views over
    // ONE `ArrayBuffer` is a legal program — that aliasing is the point of the
    // class — so this is a case a caller reaches, not a defensive check.
    for (at, first) in found.iter().enumerate() {
        for second in found.iter().skip(at + 1) {
            let ends_before = first.0.wrapping_add(first.1) <= second.0;
            let starts_after = second.0.wrapping_add(second.1) <= first.0;
            if !ends_before && !starts_after {
                return None;
            }
        }
    }
    Some(found)
}

/// # Safety
///
/// The window must be live, aligned for `f32`, a whole number of them long, and
/// not aliased by any other slice in scope. [`windows`] establishes all four.
unsafe fn floats<'a>(window: (*mut u8, usize)) -> &'a [f32] {
    unsafe { std::slice::from_raw_parts(window.0.cast::<f32>(), window.1 / 4) }
}

/// # Safety
///
/// As [`floats`], and the window must additionally be one no other live slice
/// reads — which is the disjointness [`windows`] checks.
unsafe fn floats_mut<'a>(window: (*mut u8, usize)) -> &'a mut [f32] {
    unsafe { std::slice::from_raw_parts_mut(window.0.cast::<f32>(), window.1 / 4) }
}
