//! Which callees the door may call without its argument bookkeeping.
//!
//! # What this removes, measured
//!
//! Every JavaScript call goes through `functions::called`: a class-constructor
//! check, two argument stacks pushed and popped, the callee recorded and
//! resolved, the jump, and the tail-call settle. `docs/codegen/
//! native-call-floor.md` §7b prices the bookkeeping at 7 to 9 ns of a 23 to 27
//! ns call, and records three attempts at REARRANGING it, all refuted by the
//! bench. What is left is removing work: a callee that reads none of what the
//! two argument stacks hold does not need them pushed.
//!
//! # Which callee is light, and who decides
//!
//! The COMPILER, per function, and it says so where the function becomes a
//! value: a light function's closure is made by [`closure_new_light`], which
//! records the code address before making it. `rts-codegen`'s
//! `emit/light_call.rs` has the rule and its reasons. In short: nothing in the
//! body may read the activation's argument record — `arguments`, a rest
//! parameter, a fifth parameter, `new.target` — and the function may not park.
//! What the door adds is what only it can see: that the callee is not a class
//! constructor reached without `new`.
//!
//! # Two designs that were built first, and why each lost
//!
//! **The compiled code making the jump itself.** Three small entries — enter,
//! code, leave — and an indirect call between them, behind a new machine
//! instruction. It measured 26.7 → 18.9 ns on a light callee, and it cost every
//! OTHER callee two crossings before the door it still had to take: `call
//! arguments object` 23.8 → 29.3, `call through a variable` 20.8 → 26.2 on
//! `bench/analytic.ts`, and every native called by value with them. The
//! measurement said where the 7 ns had come from: not from the crossing, which
//! that design tripled, but from the stacks it skipped. So the skip moved INTO
//! the door — the same gain for a light callee, no crossing added for anyone.
//!
//! **The answer carried as a bit of the arity** in the function table, to avoid
//! a sixth field in a tuple four formats carry. It made that table hold
//! something that was not an arity, and `rts-host`'s `aot_object.rs` — which
//! reads a declared function's arity from it — said so at the gate. In the code
//! it needs no table at all.
//!
//! # Why a set of its own and not a field of `callables`
//!
//! `callables` is read by a dozen natives through a three-field tuple, and
//! `native-call-floor.md` §3a-i measured what rearranging a structure on this
//! path costs: the ladder wins and unrelated rows lose. A set beside it adds
//! one probe to the door and changes nothing any other path reads.

use super::with_current;

/// The code addresses of the functions the compiler marked light.
///
/// Open addressing over a power-of-two table, because this is asked on every
/// call and the standard hasher costs more than what the answer saves.
#[derive(Default)]
pub(super) struct CodeSet {
    slots: Vec<u64>,
    held: usize,
}

impl CodeSet {
    /// Records one. Zero is "empty" here and is never a code address.
    pub(super) fn insert(&mut self, code: u64) {
        if code == 0 || self.contains(code) {
            return;
        }
        if (self.held + 1) * 2 > self.slots.len() {
            let wider = (self.slots.len() * 2).max(64);
            let before = std::mem::replace(&mut self.slots, vec![0; wider]);
            self.held = 0;
            for code in before.into_iter().filter(|code| *code != 0) {
                self.place(code);
            }
        }
        self.place(code);
    }

    fn place(&mut self, code: u64) {
        let mask = self.slots.len() - 1;
        let mut at = Self::start(code) & mask;
        loop {
            match self.slots[at] {
                0 => {
                    self.slots[at] = code;
                    self.held += 1;
                    return;
                }
                found if found == code => return,
                _ => at = (at + 1) & mask,
            }
        }
    }

    /// Whether `code` was recorded.
    pub(super) fn contains(&self, code: u64) -> bool {
        if self.slots.is_empty() || code == 0 {
            return false;
        }
        let mask = self.slots.len() - 1;
        let mut at = Self::start(code) & mask;
        loop {
            match self.slots[at] {
                0 => return false,
                found if found == code => return true,
                _ => at = (at + 1) & mask,
            }
        }
    }

    /// Code is aligned, so its low bits say nothing; the multiply spreads the
    /// rest.
    fn start(code: u64) -> usize {
        ((code >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32) as usize
    }
}

/// A function as a value, for one the compiler found light: the address is
/// recorded, then the closure is made exactly as [`super::closure_new`] makes
/// any other.
#[rtse::entry]
pub fn closure_new_light(code: i64, environment: u64) -> u64 {
    with_current(|context| context.light_codes.insert(code as u64));
    super::functions::closure_new(code, environment)
}

#[cfg(test)]
mod tests {
    use super::CodeSet;

    #[test]
    fn a_set_answers_for_what_it_was_given_and_nothing_else() {
        let mut set = CodeSet::default();
        assert!(!set.contains(0x1000));
        let codes: Vec<u64> = (1..500u64).map(|at| 0x7ff6_0000_0000 + at * 48).collect();
        for code in &codes {
            set.insert(*code);
        }
        for code in &codes {
            assert!(set.contains(*code), "{code:#x} was inserted");
            assert!(!set.contains(code + 8), "{:#x} was not", code + 8);
        }
        assert!(!set.contains(0));
        for code in &codes {
            set.insert(*code);
        }
        assert!(set.contains(codes[0]), "inserting what is held changes nothing");
    }
}
