//! The chain of frames a thread is standing in, from the frame-pointer links.
//!
//! [`CodeMap`] answers *which function is this address*. This answers *which
//! addresses are on the stack*, and the two together are a stack trace. Neither
//! is one alone: a map with no walk has nothing to attribute, and a walk with no
//! map is a list of hexadecimal.
//!
//! [`CodeMap`]: super::CodeMap
//!
//! # Why the frame-pointer chain and not unwind information
//!
//! `docs/engine/the-unwired-keystone.md` concluded the opposite — that the chain
//! has to come from unwind tables, because a frame between two compiled ones may
//! be a host frame that kept no frame pointer. `docs/engine/
//! what-the-literature-does-not-buy.md` then found two things that reverse it,
//! and this module is written to the second document:
//!
//! - **The unwind tables do not exist.** That document's finding 2 records it:
//!   this crate's `unwind/` is the planner for protected regions and has nothing
//!   to do with `.pdata`/`.xdata`, and neither code-generator destination emits
//!   or registers unwind information. An unwinder would treat every compiled
//!   function as a leaf and read the wrong word as a return address — wrong for
//!   any frame that has locals, which is all of them.
//! - **A production engine walks exactly this case with two loads per frame.**
//!   `preserve_frame_pointers` is set for compiled code (`target/mod.rs`), and a
//!   host frame is crossed rather than decoded: the caller records where the
//!   crossing is and the walk resumes past it.
//!
//! # Why the reads are a parameter and not an `unsafe` block in here
//!
//! Because a walk whose only instrument is a real thread's stack is a walk that
//! can be tested on one stack, in one process, in one build — and this crate's
//! rule 3 is that every module is exercisable with no client present. The chain
//! logic is the part with the invariants worth checking, so it takes a reader
//! and a test hands it a fabricated stack with a loop, a hole and a garbage
//! link in it.
//!
//! The one unsafe read belongs to whoever owns the thread, which is also the
//! only party that knows the bound. [`Chain::over_slice`] is what a test uses,
//! and a caller with a real stack passes its own reader.

/// One frame, as its links describe it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Frame {
    /// Where this frame's links are: `[fp]` is the next frame's, `[fp + 8]` is
    /// the address control returns to.
    pub frame_pointer: usize,
    /// The address control returns to when this frame ends.
    ///
    /// This is what a map attributes, and it is deliberately NOT the address of
    /// the call: a return address points at the instruction *after* it. A
    /// consumer wanting the call site subtracts one before attributing, because
    /// a call that is the last instruction of a range would otherwise attribute
    /// to whatever follows. [`Frame::call_site`] is that, in the one place.
    pub return_address: usize,
}

impl Frame {
    /// The address to attribute this frame to.
    ///
    /// One less than the return address, which is inside the call instruction
    /// rather than after it. The difference matters exactly when a call is the
    /// last instruction of its function, and then it matters completely: the
    /// frame is attributed to the next function in memory, which is a trace that
    /// names a function the program never called.
    pub fn call_site(&self) -> usize {
        self.return_address.saturating_sub(1)
    }
}

/// How many frames a walk will report before it gives up.
///
/// A chain is read from memory the walk does not own, so a cycle is
/// representable however carefully the links are checked — and a cycle in an
/// iterator that something collects is a hang rather than a wrong answer.
/// [`Chain`] also refuses a link that does not move outward, which catches every
/// cycle this has been given; the cap is what makes the refusal unnecessary
/// rather than load-bearing.
pub const FRAME_LIMIT: usize = 256;

/// A walk over the frame-pointer chain.
///
/// # The three refusals, and what each one is for
///
/// Every one of them ends the walk rather than skipping a frame, because a link
/// that fails any of them means the chain is no longer the chain:
///
/// - **outward only.** The stack grows downward, so an enclosing frame sits at a
///   HIGHER address. A link that does not increase is either a cycle or a word
///   that is not a frame pointer, and following it is unbounded work.
/// - **inside the bound.** Past the top of the thread's stack there is no frame,
///   and reading further is reading something else.
/// - **aligned.** A frame pointer is 8-aligned by the convention that wrote it.
///   An unaligned word is data that happens to look like an address.
pub struct Chain<R> {
    at: usize,
    high: usize,
    left: usize,
    read: R,
}

impl<R: FnMut(usize) -> Option<u64>> Chain<R> {
    /// A walk from `frame_pointer` outward, bounded above by `high`.
    ///
    /// `high` is the top of the thread's stack — the first address that is no
    /// longer stack. A caller that does not know it has nothing to pass and must
    /// not guess: an unbounded walk reads whatever is mapped past the stack and
    /// reports frames that do not exist.
    pub fn new(frame_pointer: usize, high: usize, read: R) -> Self {
        Chain {
            at: frame_pointer,
            high,
            left: FRAME_LIMIT,
            read,
        }
    }
}

impl<R: FnMut(usize) -> Option<u64>> Iterator for Chain<R> {
    type Item = Frame;

    fn next(&mut self) -> Option<Frame> {
        if self.left == 0 || !plausible(self.at, self.high) {
            return None;
        }
        self.left -= 1;
        let next = usize::try_from((self.read)(self.at)?).ok()?;
        let return_address = usize::try_from((self.read)(self.at.checked_add(8)?)?).ok()?;
        if return_address == 0 {
            return None;
        }
        let frame = Frame {
            frame_pointer: self.at,
            return_address,
        };
        // OUTWARD ONLY, and checked before it is stored rather than at the top of
        // the next call: a link that does not move outward ends the walk here,
        // where the frame it came from is still reportable. Checking it on entry
        // would discard this frame to refuse the next one.
        self.at = match next > self.at {
            true => next,
            false => 0,
        };
        Some(frame)
    }
}

/// Whether an address can be a frame pointer at all.
fn plausible(at: usize, high: usize) -> bool {
    at != 0 && at % 8 == 0 && at < high && high.saturating_sub(at) >= 16
}

impl<'a> Chain<Box<dyn FnMut(usize) -> Option<u64> + 'a>> {
    /// A walk over a fabricated stack, for a test.
    ///
    /// `words` is the stack as words, with `base` the address of `words[0]`. The
    /// bound is the address one past the last word, which is what the top of a
    /// real stack is.
    ///
    /// This exists so that the invariants above are checked against a chain
    /// somebody wrote on purpose — a cycle, a hole, an unaligned link — which no
    /// real thread will produce on demand. Rule 3: the module is exercisable with
    /// no client present.
    pub fn over_slice(frame_pointer: usize, base: usize, words: &'a [u64]) -> Self {
        let high = base + words.len() * 8;
        Chain::new(
            frame_pointer,
            high,
            Box::new(move |address: usize| {
                let offset = address.checked_sub(base)?;
                if offset % 8 != 0 {
                    return None;
                }
                words.get(offset / 8).copied()
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two words per frame, `[fp]` then `[fp + 8]`, as the prologue writes them.
    const BASE: usize = 0x1_0000;

    fn address_of(index: usize) -> usize {
        BASE + index * 8
    }

    /// A chain of three frames is reported innermost first, with its return
    /// addresses.
    ///
    /// The claim is the ORDER as much as the contents: a trace printed outermost
    /// first reads like a different program, and the caller is the one that
    /// reverses it.
    #[test]
    fn a_chain_is_walked_from_the_inside_out() {
        //   index 0,1 -> innermost frame: links to index 2, returns to 0xAA
        //   index 2,3 -> middle:          links to index 4, returns to 0xBB
        //   index 4,5 -> outermost:       links to 0,       returns to 0xCC
        let words = [
            address_of(2) as u64,
            0xAA,
            address_of(4) as u64,
            0xBB,
            0,
            0xCC,
        ];
        let walked: Vec<_> = Chain::over_slice(address_of(0), BASE, &words).collect();
        assert_eq!(
            walked.iter().map(|f| f.return_address).collect::<Vec<_>>(),
            [0xAA, 0xBB, 0xCC]
        );
        assert_eq!(walked[0].frame_pointer, address_of(0));
        assert_eq!(walked[2].frame_pointer, address_of(4));
    }

    /// A link that points back inward ends the walk instead of looping.
    ///
    /// The failure this prevents is a hang rather than a wrong trace, and it is
    /// reachable from any corrupt word: the chain is read out of memory this walk
    /// does not own.
    #[test]
    fn a_cycle_ends_the_walk() {
        // The second frame links back to the first.
        let words = [address_of(2) as u64, 0xAA, address_of(0) as u64, 0xBB];
        let walked: Vec<_> = Chain::over_slice(address_of(0), BASE, &words).collect();
        assert_eq!(
            walked.iter().map(|f| f.return_address).collect::<Vec<_>>(),
            [0xAA, 0xBB],
            "both frames are real and reportable; what must not happen is a third"
        );
    }

    /// A frame needs BOTH of its words below the bound.
    ///
    /// The off-by-one worth a test: a frame pointer one word under the top of
    /// the stack has room for its link and none for its return address, and
    /// reading the second word there reads whatever is mapped past the stack.
    /// The first version of this test asserted the wrong thing — that a frame
    /// whose words both fit was refused — and the predicate was right.
    #[test]
    fn a_frame_needs_both_words_below_the_bound() {
        let words = [address_of(2) as u64, 0xAA, 0, 0xBB];
        let read = |address: usize| {
            let offset = address.checked_sub(BASE)?;
            words.get(offset / 8).copied()
        };

        assert_eq!(
            Chain::new(address_of(0), address_of(2), read).count(),
            1,
            "a frame whose two words both fit under the bound was refused"
        );
        assert_eq!(
            Chain::new(address_of(1), address_of(2), read).count(),
            0,
            "a frame with room for its link and not its return address was              reported, so the second word came from past the stack"
        );
    }

    /// An unaligned or null link ends the walk.
    #[test]
    fn a_link_that_cannot_be_a_frame_pointer_ends_the_walk() {
        let words = [address_of(2) as u64 + 1, 0xAA, 0, 0xBB];
        let walked: Vec<_> = Chain::over_slice(address_of(0), BASE, &words).collect();
        assert_eq!(walked.len(), 1, "the unaligned link was followed");

        let null = [0u64, 0xAA];
        assert_eq!(
            Chain::over_slice(address_of(0), BASE, &null).count(),
            1,
            "a null link is the end of the chain, not a frame"
        );
        assert_eq!(
            Chain::over_slice(0, BASE, &null).count(),
            0,
            "a walk starting nowhere reported a frame"
        );
    }

    /// A return address of zero is the end rather than a frame.
    ///
    /// The outermost frame of a thread has no caller, and what sits in that slot
    /// is zero on every platform this targets. Reporting it would put an
    /// unattributable address at the end of every trace.
    #[test]
    fn a_zero_return_address_is_the_end() {
        let words = [address_of(2) as u64, 0xAA, 0, 0];
        let walked: Vec<_> = Chain::over_slice(address_of(0), BASE, &words).collect();
        assert_eq!(walked.len(), 1);
    }

    /// A read the caller cannot answer ends the walk rather than inventing a
    /// frame.
    #[test]
    fn an_unreadable_word_ends_the_walk() {
        let mut answered = 0;
        let walk = Chain::new(address_of(0), address_of(64), |_| {
            answered += 1;
            match answered {
                1 => Some(address_of(2) as u64),
                _ => None,
            }
        });
        assert_eq!(walk.count(), 0, "a frame was reported from one word");
    }

    /// A chain longer than the cap stops at the cap.
    ///
    /// Deep recursion is ordinary in a program and a trace does not need all of
    /// it; what the cap guarantees is that a walk terminates even where the
    /// outward-only rule is satisfied by a long run of plausible words.
    #[test]
    fn the_walk_is_bounded_by_the_frame_limit() {
        let count = FRAME_LIMIT * 2;
        let mut words = Vec::new();
        for index in 0..count {
            words.push(address_of((index + 1) * 2) as u64);
            words.push(0xAA + index as u64);
        }
        let walked = Chain::over_slice(address_of(0), BASE, &words).count();
        assert_eq!(walked, FRAME_LIMIT);
    }

    /// The address a frame is attributed to is inside the call, not after it.
    #[test]
    fn a_frame_is_attributed_to_the_call_and_not_to_what_follows() {
        let frame = Frame {
            frame_pointer: address_of(0),
            return_address: 0x4000,
        };
        assert_eq!(frame.call_site(), 0x3fff);
    }
}
