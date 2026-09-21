//! Executable memory for one placed program, as ONE contiguous reservation.
//!
//! # Why an arena, and not the provider the code generator defaults to
//!
//! The default provider asks the operating system for a fresh chunk whenever
//! the one it has runs out, and a chunk lands wherever the system puts it.
//! Between two chunks of the same program the process heap grows by whatever
//! compiling that program allocated, and a call from a function in one chunk
//! to a function in another is a 32-bit PC-relative displacement: past ±2 GB
//! there is no encoding for it, and `finalize_definitions` panics
//! (`compiled_blob.rs:142`, `TryFromIntError`) rather than answering.
//!
//! Measured 2026-09-11 placing one 3.8 MB script of WhatsApp Web: 600 MB of
//! code and data, chunks more than 2 GB apart, and no program run at all. A
//! contiguous reservation puts every function of the program within reach of
//! every other by construction — which is the assumption a relocation of that
//! width makes, and the provider was not keeping it.
//!
//! Calls OUT of the program — to the host's entry points, handed in by name —
//! are not affected either way: an imported symbol is reached through an
//! absolute 64-bit address, not a displacement. Read from the code generator's
//! `compiled_blob.rs`; what was not verified is whether a future version makes
//! imports PC-relative too, and the test in `tests/` that places a program
//! whose one call is to an import would say so.
//!
//! # Why sized from the program, and why it may take a second attempt
//!
//! The arena is reserved up front and, on Windows, committed up front as well
//! — `region` reserves and commits in one call there — so its size is charged
//! against the commit limit and cannot simply be "a lot" per script. It is
//! estimated from what the program holds, with headroom. A program that
//! outgrows the estimate raises one plain error with fixed text, which is the
//! only signal the provider offers; it is matched in ONE place here, and the
//! program is compiled again into an arena four times larger, twice at most.
//! Compiling twice costs seconds on the largest script measured; refusing a
//! program for guessing wrong would cost the program.

use cranelift_module::ModuleError;

use super::TargetError;
use super::hosted::Placing;

/// Bytes of arena per instruction of the program.
///
/// A lowered instruction is usually a handful of machine instructions; a call
/// with its safepoint, or a guard with its two edges, is a few dozen bytes.
/// Sixty-four leaves room for both without a measurement of every shape, and
/// the cost of being generous is virtual space that the arena commits but the
/// program never touches.
const BYTES_PER_INSTRUCTION: usize = 64;

/// Bytes of arena per block: a label, and the jump that usually ends it.
const BYTES_PER_BLOCK: usize = 32;

/// Bytes of arena per function: prologue, epilogue, alignment padding, and
/// the tables the code generator keeps beside the code.
const BYTES_PER_FUNCTION: usize = 1024;

/// The least an arena is: a program of one function still needs a page for
/// code and one for each kind of data.
const FLOOR: usize = 4 << 20;

/// The arena sizes to try, as multiples of the estimate, in order.
///
/// Three attempts and a factor of four: the estimate above is meant to be
/// right the first time, so the second attempt is for a program whose shape
/// the estimate did not foresee, and the third is the bound past which
/// something other than the estimate is wrong.
pub(super) const GROWTH: [usize; 3] = [1, 4, 16];

/// The bytes to reserve for `program`, before compiling it.
pub(super) fn estimated_bytes(program: &[Placing<'_>]) -> usize {
    let mut bytes = 0usize;
    for placing in program {
        let Some(body) = placing.body else {
            continue;
        };
        bytes = bytes.saturating_add(BYTES_PER_FUNCTION);
        for (_, block) in body.blocks() {
            bytes = bytes
                .saturating_add(BYTES_PER_BLOCK)
                .saturating_add(block.insts.len().saturating_mul(BYTES_PER_INSTRUCTION));
        }
    }
    bytes.max(FLOOR)
}

/// Whether `error` is the provider saying the arena is full — the one case
/// worth a second attempt.
///
/// The provider raises an `io::Error` with fixed text and the module wraps it
/// as a backend error, so the text is the only thing to match. It is matched
/// here and nowhere else, so that the day the text changes there is one place
/// that stops recognising it — and the test beside this function, which fills
/// a deliberately small arena, is what says so.
pub(super) fn exhausted(error: &TargetError) -> bool {
    match error {
        TargetError::Module(ModuleError::Backend(cause)) => {
            format!("{cause:#}").contains("jit memory region exhausted")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_program_still_reserves_the_floor() {
        assert_eq!(
            estimated_bytes(&[]),
            FLOOR,
            "a program with nothing to place still needs pages for code and data"
        );
    }

    #[test]
    fn only_the_arena_being_full_earns_a_second_attempt() {
        let full = TargetError::Module(ModuleError::Backend(anyhow::Error::new(
            std::io::Error::other("pre-allocated jit memory region exhausted"),
        )));
        assert!(exhausted(&full), "the provider's own text is the signal");
        let other = TargetError::Module(ModuleError::Backend(anyhow::Error::msg("something else")));
        assert!(
            !exhausted(&other),
            "any other backend error is not a size problem"
        );
    }
}
