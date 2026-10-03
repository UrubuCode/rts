//! The running call stack, read from the machine stack instead of from
//! `callees`.
//!
//! # What this is for, and it is not a tidier trace
//!
//! `docs/codegen/native-call-floor.md` ranks a direct call to a statically known
//! callee as the largest remaining item on the call path — about 16 ns of a 23 ns
//! call — and names the one thing that blocks it: a direct call does not push
//! `callees`, so every frame it skips disappears from `new Error().stack`.
//! `throw::stack_text_of` walks that list, and nothing else knows what is
//! running.
//!
//! So this is the keystone rather than a feature: once a trace comes from the
//! machine stack, a call no longer has to announce itself to be visible, and
//! `callees` stops being the only witness.
//!
//! # How it works, and the three tables it does NOT duplicate
//!
//! `rts_cranelift::observe::Chain` walks the frame-pointer links and hands back
//! return addresses. Turning one into a name is two lookups over tables that
//! already exist, and deliberately no new one:
//!
//! 1. [`Context::code_map`] answers *which function contains this address*, by
//!    bisection, because a return address is in the middle of a function and the
//!    other table is keyed by entry addresses.
//! 2. That range's START is an entry address, which is exactly what
//!    `function_names` is keyed by — so the NAME comes from the table that was
//!    already the authority for it.
//!
//! The second step is the point. The map carries a name of its own, and using it
//! would make two tables answer "what is this function called" — P1, with the
//! two able to disagree the first time either learned a convention. The map is
//! used for the range and nothing else.
//!
//! # Why this runs beside `callees` rather than replacing it
//!
//! `docs/engine/the-unwired-keystone.md` prescribes the order for exactly this
//! kind of change, about the collector's roots: build the precise mechanism,
//! keep the old one running, and compare them on a real program before anything
//! depends on the new one. A trace is cheaper to be wrong about than a root set,
//! but the argument is the same — and here it is checkable, because the two
//! mechanisms answer the same question in the same words.
//!
//! `RTS_TRACE_COMPARE=1` prints both for every captured trace. Nothing reads
//! this module's answer yet, and the measurement below is why.
//!
//! # MEASURED 2026-10-03: the walk cannot START in a host frame
//!
//! On the program
//!
//! ```text
//! function inner() { return new Error("here").stack }
//! function middle() { return inner() }
//! function outer() { return middle() }
//! ```
//!
//! the comparison answers `callees: ["inner"]` and `machine: [] (1 frames
//! walked, 0 attributed)`. The link dump says why, and it is not the map:
//!
//! ```text
//! link fp=0xf3e73ebda0 next=0x24d2be01860 ret=0x24d2bdffcb0 high=0xf3e7400000
//! ```
//!
//! `fp` is a stack address and `next` is a HEAP address, so the word at `[rbp]`
//! is not a saved frame pointer. `rbp` is a callee-saved register here and
//! nothing has made it a frame pointer: `registers.rs` captures it as a VALUE
//! for the collector, which works whatever it holds, and this needs it to be a
//! link. `RUSTFLAGS="-C force-frame-pointers=yes"` was tried and changes
//! nothing — which also refutes the remedy
//! `what-the-literature-does-not-buy.md` offers for this exact premise.
//!
//! So the keystone document's original worry was right about the host frames
//! and wrong about the reason: the problem is not that a host frame KEEPS no
//! frame pointer in the middle of the chain, it is that the walk has no
//! trustworthy place to BEGIN, because every capture point is a host frame.
//!
//! # What that leaves, and it is one machine capability
//!
//! The engines cited by that document do not start in a host frame either.
//! They record the crossing: V8 writes `c_entry_fp_`, JSC `VM::topCallFrame`,
//! wasmtime `last_wasm_exit_fp`. **Compiled code stores its own frame pointer
//! where the runtime can find it, before it calls out** — and from there the
//! chain is all compiled frames, which DO link, because
//! `preserve_frame_pointers` is set for them.
//!
//! That is a machine capability and not a runtime one: it is a store of the
//! frame pointer, which only the layer that emits prologues can name. Per
//! ACTIVATION rather than per call is what makes it cheap — three stores at a
//! boundary, which is also what makes the direct call worth having, since a
//! compiled-to-compiled call then records nothing at all.

use rts_cranelift::observe::Chain;

use super::Context;

/// This frame's frame pointer, from the register that holds it.
///
/// # Why a capture and not an argument
///
/// Because the caller is a native in the middle of the runtime and the thing
/// wanted is the chain it is standing on. `super::registers::callee_saved`
/// captures the same register for the collector and for the same reason — a
/// value that exists only in a register is invisible to anything reading memory.
///
/// `None` on a target this has not been written for, which is honest rather than
/// a guess: a wrong frame pointer walks something that is not the stack.
#[inline(always)]
pub(super) fn frame_pointer() -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    {
        let mut held: u64 = 0;
        // SAFETY: one `mov` from a register into a local, writing nothing else,
        // reading and setting no flags, pushing nothing — which is what
        // `nostack` and `preserves_flags` assert. The same shape as
        // `registers::captured`, which has carried it since 2026-08-24.
        unsafe {
            core::arch::asm!(
                "mov {out}, rbp",
                out = out(reg) held,
                options(nostack, preserves_flags, nomem),
            );
        }
        Some(held as usize)
    }
    #[cfg(target_arch = "aarch64")]
    {
        let mut held: u64 = 0;
        // SAFETY: as above. `x29` is the frame pointer on this target, which
        // `registers.rs` states while listing the non-volatile set.
        unsafe {
            core::arch::asm!(
                "mov {out}, x29",
                out = out(reg) held,
                options(nostack, preserves_flags, nomem),
            );
        }
        Some(held as usize)
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        None
    }
}

/// The compiled functions currently running, innermost first.
///
/// `None` where the walk cannot be taken at all — no code map installed, no
/// stack bound from the host, or a target with no frame pointer this knows. An
/// empty vector is a different answer: the walk ran and attributed nothing,
/// which is what a stack of nothing but host frames looks like.
///
/// # Why unattributed frames are skipped and not reported
///
/// Between two compiled frames there are host frames — `call_counted`, `called`,
/// `invoke`, and whatever native is running — and they are not part of the
/// program's own call stack. `the-unwired-keystone.md` names this as the design
/// decision: **the walk must SKIP rather than stop**, because a walker that
/// ended at the first unattributed address would report exactly one frame.
///
/// # Why it also reports what it SAW
///
/// The two counts are the diagnostic that matters while this runs beside
/// `callees`: an empty answer from a walk that saw no frames at all is a broken
/// chain, and an empty answer from a walk that saw forty is a map that attributes
/// none of them. Those are different defects and the trace alone cannot tell
/// them apart.
pub(in crate::entry) fn census(context: &Context) -> Option<(Vec<String>, usize, usize)> {
    let map = context.code_map.as_ref()?;
    let high = context.stack_high?;
    let from = frame_pointer()?;

    // THE FIRST LINKS, VERBATIM, under the same switch.
    //
    // This is the instrument that found what the measurement below records, and
    // it stays for the reason it was needed: `0 attributed` says nothing about
    // WHY, and the first thing to know is whether the words being followed are
    // frame links at all. Six is enough to tell a chain that walks from one that
    // leaves the stack on its first step.
    if comparing() {
        let mut at = from;
        for _ in 0..6 {
            if at == 0 {
                break;
            }
            // SAFETY: the same argument the walk below makes, over the same
            // range, and this loop is reached only under the switch.
            let (next, ret) = unsafe {
                (
                    std::ptr::read(at as *const u64),
                    std::ptr::read((at + 8) as *const u64),
                )
            };
            eprintln!("  rts-trace link fp={at:#x} next={next:#x} ret={ret:#x} high={high:#x}");
            at = next as usize;
        }
    }
    let mut named = Vec::new();
    let mut walked = 0usize;
    let mut attributed = 0usize;
    // SAFETY: every read is of a word inside `from..high`. `high` came from the
    // host's own OS call for THIS thread (`rts-host`'s `stack.rs`), and `Chain`
    // refuses an address that is not 8-aligned, does not move outward, or has no
    // room for both of a frame's words below the bound. The same argument
    // `collect_cycle` makes for scanning the stack conservatively, over the same
    // range.
    let walk = Chain::new(from, high, |address: usize| unsafe {
        Some(std::ptr::read(address as *const u64))
    });
    for frame in walk {
        walked += 1;
        let Some((range, _)) = map.at(frame.call_site()) else {
            continue;
        };
        // THE NAME COMES FROM `function_names`, through the range's entry
        // address, and not from the range itself — see the module header for why
        // two tables answering one question is the failure being avoided here.
        // The same lookup `throw::stack_text_of` makes, over the same table, so
        // that the two mechanisms are comparable in the only way that matters:
        // if they disagree it is about which FRAMES are running, never about
        // what one of them is called.
        if let Some((_, text, _, _, _)) = context
            .function_names
            .iter()
            .find(|(at, name, _, _, _)| *at == range.start as u64 && !name.is_empty())
        {
            named.push(text.clone());
        }
    }
    Some((named, walked, attributed))
}

/// Prints both mechanisms side by side, for one capture.
///
/// Called from every point that records frames — `Context::defer_stack` for
/// `new Error` and `error_stack::kept` for `captureStackTrace` — so the
/// comparison is one function rather than one per capture. Two copies would
/// disagree about which list they reversed, which is the only thing a reader
/// of this output is looking at.
///
/// Here and not in the renderer, which is the subtlety worth stating: an
/// `Error` records its frames where it is CONSTRUCTED and renders them only if
/// something reads `.stack`, by which time the live stack has unwound. A walk
/// taken at render time would describe the renderer.
pub(in crate::entry) fn compare(context: &Context, frames: &[u64]) {
    if !comparing() {
        return;
    }
    let from_callees: Vec<String> = frames
        .iter()
        .rev()
        .filter_map(|callee| {
            let cell = crate::value::Value(*callee).as_slot()?;
            let (code, _) = context.callable_at(cell)?;
            context
                .function_names
                .iter()
                .find(|(at, name, _, _, _)| *at == code && !name.is_empty())
                .map(|(_, name, _, _, _)| name.clone())
        })
        .collect();
    eprintln!("rts-trace callees:  {from_callees:?}");
    match census(context) {
        Some((named, walked, attributed)) => eprintln!(
            "rts-trace machine:  {named:?}  ({walked} frames walked, {attributed} attributed)"
        ),
        None => eprintln!(
            "rts-trace machine:  unavailable (code map {}, stack bound {})",
            context.code_map.is_some(),
            context.stack_high.is_some()
        ),
    }
}

/// Whether to print both traces for every capture.
///
/// The switch the keystone document's ordering asks for: the new mechanism runs
/// beside the old one and the two are compared on a real program before
/// anything depends on the new one.
pub(super) fn comparing() -> bool {
    std::env::var("RTS_TRACE_COMPARE").is_ok_and(|held| held != "0")
}
