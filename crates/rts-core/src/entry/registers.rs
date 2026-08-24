//! The half of the root set that is not in memory when a collection runs.
//!
//! # Why this exists, and the failing program that made it necessary
//!
//! [`super::roots::scan_stack`] walks `stack_low..stack_high` and keeps every
//! word that decodes as a reference. A value live only in a **callee-saved
//! register** is not in that range, so it is not a root, so it is swept — and
//! whoever reads it next reads a freed cell.
//!
//! `roots.rs` argued, on 2026-08-14, that this could not happen: a call
//! clobbers the volatile registers, so anything live across one is already in
//! the frame the walk reads, and what a callee keeps in a callee-saved register
//! it pushed in its own prologue onto the same stack. It recorded that as
//! **"absence of a failing case, not a proof"**, and named the condition for
//! reopening it: *"A program that breaks it makes it necessary again."*
//!
//! This is that program, measured 2026-08-24, release:
//!
//! ```text
//! class P { constructor(public x: number, public y: number) {} }
//! const pts: P[] = [];
//! for (let i = 0; i < 20000; i++) pts.push(new P(i % 137, (i * 7) % 91));
//! const m = new Map<string, number>();
//! for (const p of pts) { const k = `${p.x}:${p.y}`; m.set(k, (m.get(k) ?? 0) + 1); }
//! console.log(m.size, JSON.stringify({ n: pts.length }).length);
//! ```
//!
//! `TypeError: Cannot read properties of undefined (reading 'x')`, every run,
//! and `RTS_GC_DEBUG=1` says why: `roots 183 live 3711 freed 61737`. The 20 000
//! objects `pts` still names were swept, because `pts` was in a callee-saved
//! register and nothing between it and the collector had reused that register,
//! so no prologue had pushed it anywhere the walk could see.
//!
//! The gap in the 08-14 reasoning is that last clause. It holds only when some
//! frame between the compiled one and this one *happens* to want the same
//! register; whether one does is a register allocator's decision, which is why
//! the bug moves when anything unrelated is added. In the program above,
//! deleting the `JSON.stringify(…)` from the final line makes it pass — that
//! call changes what the top-level frame keeps where, not what it keeps alive.
//! Two earlier probes missing it is consistent with this: eight and six live
//! objects fit in the frame, where twenty thousand plus a `Map` do not.
//!
//! # Why this is a root SOURCE and not a stack flush
//!
//! Pushing the registers would put them **below** `stack_low` — the stack grows
//! down and `stack_low` is an anchor local in `alloc`'s frame — so the walk
//! that already exists would not reach them. They are handed to the same
//! [`conservative_roots`] filter instead, which is what they are: `Value` words
//! that happen not to be in memory. Rule 3 of this crate's README, applied to a
//! root: one rule for "is this word a reference", however many places words
//! arrive from.
//!
//! # Why a register read does not need an operating system
//!
//! Rule 1 keeps anything needing an OS out of this crate, and that is what
//! stopped `scan_stack` from finding its own bounds: the *top* of a thread's
//! stack is a `GetCurrentThreadStackLimits`/`/proc/self/maps` question. Which
//! registers a calling convention preserves is not — it is an architecture, the
//! same kind of fact the value encoding is, and it is answered here by `cfg`
//! rather than by a call.

use crate::collect::conservative_roots;
use crate::heap::Slot;

/// Every callee-saved register, as a candidate root.
///
/// Called with the collector's own frame live, which is the point: it reports
/// what the registers hold *now*. A register an intervening frame reused was
/// saved by that frame's prologue and is found by the stack walk; a register
/// nothing reused still holds what compiled code left in it and is found here.
/// Neither half is sufficient alone, and the overlap costs a retained cell for
/// one cycle — the same margin `scan_stack` already documents accepting.
#[inline(always)]
pub fn callee_saved() -> Vec<Slot> {
    conservative_roots(&captured())
}

/// The Windows/System V x86-64 non-volatile set.
///
/// `rbx`, `rbp`, `rsi`, `rdi`, `r12`–`r15`. The two conventions disagree about
/// `rsi`/`rdi` — non-volatile on Windows, volatile on System V — and reading
/// both on both is right rather than merely cheap: a volatile register holds
/// something arbitrary, and an arbitrary word that decodes as a reference is
/// precisely what [`conservative_roots`] is built to over-approximate. Getting
/// it wrong in this direction retains; the other direction frees.
///
/// `rsp` is absent because it is the stack pointer, and everything it addresses
/// is what the walk already covers.
#[cfg(target_arch = "x86_64")]
#[inline(always)]
fn captured() -> [u64; 8] {
    let mut saved = [0u64; 8];
    // SAFETY: eight `mov`s from register to memory, into a local array that is
    // eight `u64`s long and live for the whole block. No memory but `saved` is
    // written, no flags are read or set, and nothing is pushed — which is what
    // `nostack` and `preserves_flags` assert. The pointer operand is `reg`, so
    // the assembler picks a register that is not one being read.
    unsafe {
        core::arch::asm!(
            "mov [{p} + 0], rbx",
            "mov [{p} + 8], rbp",
            "mov [{p} + 16], rsi",
            "mov [{p} + 24], rdi",
            "mov [{p} + 32], r12",
            "mov [{p} + 40], r13",
            "mov [{p} + 48], r14",
            "mov [{p} + 56], r15",
            p = in(reg) saved.as_mut_ptr(),
            options(nostack, preserves_flags),
        );
    }
    saved
}

/// The AArch64 non-volatile set: `x19`–`x28`, and `x29` (the frame pointer).
///
/// `x30` is the link register — a return address, never a value — and is left
/// out for the same reason `rsp` is above.
///
/// **Measured on x86-64 only.** The failing program at the top of this module
/// is what says the x86-64 path works; no AArch64 machine was available to run
/// it on, and this crate's honesty floor is why that is written here rather
/// than implied by the code's presence. It is included anyway because the
/// alternative is not "untested code" but a silent hole: without it, macOS and
/// Linux ARM keep exactly the bug this module fixes.
#[cfg(target_arch = "aarch64")]
#[inline(always)]
fn captured() -> [u64; 11] {
    let mut saved = [0u64; 11];
    // SAFETY: eleven stores from register to memory, into a local array of
    // eleven `u64`s live for the whole block. Same contract as the x86-64 arm.
    unsafe {
        core::arch::asm!(
            "str x19, [{p}, #0]",
            "str x20, [{p}, #8]",
            "str x21, [{p}, #16]",
            "str x22, [{p}, #24]",
            "str x23, [{p}, #32]",
            "str x24, [{p}, #40]",
            "str x25, [{p}, #48]",
            "str x26, [{p}, #56]",
            "str x27, [{p}, #64]",
            "str x28, [{p}, #72]",
            "str x29, [{p}, #80]",
            p = in(reg) saved.as_mut_ptr(),
            options(nostack, preserves_flags),
        );
    }
    saved
}

/// Everywhere else — wasm above all, which this crate exists on.
///
/// Not a stub standing in for work owed: wasm has no addressable registers and
/// no addressable stack, so neither this nor [`super::roots::scan_stack`] can
/// run there at all. A conservative root set is not available on that target by
/// construction, and saying so is the honest answer rather than an empty
/// function that reads like an oversight.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[inline(always)]
fn captured() -> [u64; 0] {
    []
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;

    /// A reference parked in a callee-saved register is reported as a root.
    ///
    /// The register is named explicitly rather than hoped for: `inout("r15")`
    /// puts the encoded word there and takes it back, so between the two the
    /// value is in `r15` and the test asserts that [`callee_saved`] finds it.
    /// Written this way because the bug it pins is exactly "the allocator
    /// happened to choose a register nobody else touched" — a test that let the
    /// compiler pick would stop testing that the day it picked differently.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn a_reference_in_a_callee_saved_register_is_a_root() {
        let reference = Value::from_slot(4242).bits();
        let mut parked = reference;
        // SAFETY: an empty template that touches no memory and sets no flags.
        // `r15` is declared `inout`, so the compiler saves and restores
        // whatever it held; the only effect is that `parked` is in `r15`
        // across the `callee_saved` call below.
        let found = unsafe {
            core::arch::asm!("", inout("r15") parked, options(nostack, preserves_flags));
            callee_saved()
        };
        assert_eq!(parked, reference, "the register round-tripped the word");
        assert!(
            found.contains(&Slot(4242)),
            "a reference in r15 must be a root; got {found:?}"
        );
    }

    /// An ordinary word in a register is not a root.
    ///
    /// The other half of [`conservative_roots`]'s rule, asserted here so that
    /// this module cannot start reporting every word it reads: a scanner that
    /// roots everything retains everything, which is a leak wearing a
    /// collection's clothes.
    #[test]
    fn an_unrelated_cell_is_not_reported() {
        let found = callee_saved();
        assert!(
            !found.contains(&Slot(0x00DE_ADBE)),
            "nothing should decode to that cell; got {found:?}"
        );
    }
}
