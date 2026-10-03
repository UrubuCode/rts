//! How often a site saw what — and nothing about what that means.

use crate::witness::Witness;

/// How many distinct witnesses one site records before it stops admitting new
/// ones.
///
/// # Why four, and why a limit at all
///
/// A site that has seen four different layouts is telling you something, and a
/// site that has seen four hundred is telling you the same thing more
/// expensively. Past the point where a majority is plausible, the only fact
/// worth keeping is *that* there were more — which [`Observation::overflowed`]
/// records in one bit.
///
/// Four rather than two because a polymorphic-but-bounded site is real and
/// worth seeing whole: a reader may want the top two of three rather than
/// nothing. Four rather than sixteen because the width is paid on the recording
/// path, per site, in a runtime that is already on its slow path — and because
/// no reader has yet asked for a fifth.
pub const WITNESS_WIDTH: usize = 4;

/// What one site saw, as counts.
///
/// # What is deliberately absent
///
/// Any verdict. There is no `should_speculate`, no threshold, no "is this
/// monomorphic". A frequency becomes an assumption only through
/// `rts_mir::domain::Domain`, in the front end, because what a majority
/// authorises is a judgement about one language's semantics — and a judgement
/// taken here would be one every client inherited, which is the mistake
/// `rts_cranelift::observe` declines to make about sampling policy.
///
/// So the query is [`Observation::majority`], and it answers a count beside a
/// total. The division is the reader's, and so is the threshold.
#[derive(Clone, Debug, Default)]
pub struct Observation {
    seen: Vec<(Witness, u64)>,
    total: u64,
    overflowed: bool,
}

impl Observation {
    /// A site that has seen nothing.
    pub fn new() -> Self {
        Observation::default()
    }

    /// Records one arrival.
    ///
    /// Past [`WITNESS_WIDTH`] distinct witnesses, a new one is not admitted and
    /// [`Observation::overflowed`] becomes true — but `total` still rises. That
    /// asymmetry is the point and it keeps the arithmetic honest: a recorded
    /// witness's count stays exact and the total stays exact, so its share is
    /// exact. What is lost is only the identity of the witnesses that did not
    /// fit, and a recorded witness holding a small share of a large total is
    /// precisely the site a reader should decline to speculate on.
    pub fn saw(&mut self, witness: &Witness) {
        self.total = self.total.saturating_add(1);
        if let Some(entry) = self.seen.iter_mut().find(|(seen, _)| seen == witness) {
            entry.1 = entry.1.saturating_add(1);
            return;
        }
        if self.seen.len() == WITNESS_WIDTH {
            self.overflowed = true;
            return;
        }
        self.seen.push((witness.clone(), 1));
    }

    /// Records `count` arrivals of one witness at once.
    ///
    /// What reading a record back uses, and what merging two records uses. The
    /// one-at-a-time path is not a loop over this, because the overflow rule has
    /// to behave the same whether a site was seen a thousand times in one run or
    /// a thousand times across two.
    pub fn saw_many(&mut self, witness: &Witness, count: u64) {
        if count == 0 {
            return;
        }
        self.total = self.total.saturating_add(count);
        if let Some(entry) = self.seen.iter_mut().find(|(seen, _)| seen == witness) {
            entry.1 = entry.1.saturating_add(count);
        } else if self.seen.len() == WITNESS_WIDTH {
            self.overflowed = true;
        } else {
            self.seen.push((witness.clone(), count));
        }
    }

    /// How many arrivals this site has had, including ones no witness holds.
    pub fn total(&self) -> u64 {
        self.total
    }

    /// Whether this site saw more distinct witnesses than it can hold.
    ///
    /// A reader that treats this as "do not speculate here" is making the usual
    /// choice, and it is still the reader's choice — see the type's own note on
    /// why no verdict lives here.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Every witness and its count, most frequent first.
    ///
    /// Ties broken by the witness's own order, so two runs over the same program
    /// produce the same list — `rts-cranelift` rule 13, for anything a person
    /// compares between builds.
    pub fn ranked(&self) -> Vec<(&Witness, u64)> {
        let mut ranked: Vec<_> = self.seen.iter().map(|(w, c)| (w, *c)).collect();
        ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        ranked
    }

    /// The most frequent witness, with the numbers a reader needs to judge it.
    ///
    /// `None` only when the site was never reached. A site that overflowed still
    /// has a majority among what it kept, and the caller is told so.
    pub fn majority(&self) -> Option<Majority<'_>> {
        let (witness, count) = self.ranked().into_iter().next()?;
        Some(Majority {
            witness,
            count,
            total: self.total,
            overflowed: self.overflowed,
        })
    }

    /// Adds another observation of the same site into this one.
    ///
    /// Two runs of the same program, or two threads of one. Overflow is sticky:
    /// a site that overflowed anywhere overflowed.
    pub fn absorb(&mut self, other: &Observation) {
        for (witness, count) in other.ranked() {
            self.saw_many(witness, count);
        }
        let attributed: u64 = other.ranked().iter().map(|(_, count)| *count).sum();
        let unattributed = other.total.saturating_sub(attributed);
        self.total = self.total.saturating_add(unattributed);
        self.overflowed |= other.overflowed;
    }
}

/// Two observations are equal when they say the same thing, whatever order they
/// were told in.
///
/// Derived equality would compare `seen` as a `Vec`, so a site told about its
/// witnesses in one order would differ from the same site told in another — and
/// the first thing that breaks is the round trip through [`crate::text`], which
/// writes witnesses ranked and therefore rarely in the order they arrived. An
/// equality that depends on arrival order is an equality that makes the format
/// untestable.
impl PartialEq for Observation {
    fn eq(&self, other: &Self) -> bool {
        self.total == other.total
            && self.overflowed == other.overflowed
            && self.ranked() == other.ranked()
    }
}

impl Eq for Observation {}

impl Observation {
    /// States a site's totals directly, as reading a record back does.
    ///
    /// Separate from [`Observation::saw`] because a record carries a total that
    /// its surviving witnesses do not add up to: an overflowed site counted
    /// arrivals whose witnesses it could not keep. Reconstructing by replaying
    /// the counts would lose exactly that difference and inflate every share on
    /// the sites where being wrong matters most.
    pub fn declare(&mut self, total: u64, overflowed: bool) {
        self.total = total;
        self.overflowed = overflowed;
    }

    /// Puts back one witness and its count, without touching the total.
    ///
    /// The other half of [`Observation::declare`]. Past [`WITNESS_WIDTH`] the
    /// witness is dropped and the overflow bit set, so a record that was
    /// hand-edited to carry more witnesses than a run could have produced is
    /// read as the overflowed site it describes rather than as a wider one.
    pub fn restore(&mut self, witness: &Witness, count: u64) {
        if let Some(entry) = self.seen.iter_mut().find(|(seen, _)| seen == witness) {
            entry.1 = entry.1.saturating_add(count);
        } else if self.seen.len() == WITNESS_WIDTH {
            self.overflowed = true;
        } else {
            self.seen.push((witness.clone(), count));
        }
    }
}

/// The most frequent witness at a site, and the numbers that qualify it.
///
/// Every field is a count or a fact, and none is a recommendation. A reader
/// decides what `count` out of `total` is worth, and whether `overflowed`
/// disqualifies the site — that decision is language meaning and belongs in a
/// `Domain`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Majority<'a> {
    /// What arrived most often.
    pub witness: &'a Witness,
    /// How many times it did. Exact, even if the site overflowed.
    pub count: u64,
    /// How many arrivals the site had in total. Exact.
    pub total: u64,
    /// Whether witnesses were seen that this record could not hold.
    pub overflowed: bool,
}

/// Where a site's counts are written while a program runs.
///
/// # Why a separate type from the record
///
/// Because the writing side has one job — be cheap at a point the runtime is
/// already standing on — and the reading side has another. A recorder is reached
/// from a cache miss, where the name looked for and the layout found are
/// *already in hand*: `docs/engine/profile-oracle.md` is explicit that this is
/// why no instrumented tier is needed. Keeping it apart means the query methods
/// cannot be called on the hot structure by accident, and the hot structure can
/// change shape without moving the format.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Recorder {
    observation: Observation,
}

impl Recorder {
    /// A recorder that has seen nothing.
    pub fn new() -> Self {
        Recorder::default()
    }

    /// Records one arrival.
    pub fn saw(&mut self, witness: &Witness) {
        self.observation.saw(witness);
    }

    /// What was recorded.
    pub fn finish(self) -> Observation {
        self.observation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A site that overflowed still reports an exact share for what it kept.
    ///
    /// The arithmetic is the reason the overflow bit is separate from the
    /// counts: if `total` stopped rising, a megamorphic site would report its
    /// first four witnesses as if they were the whole population, and a reader
    /// applying any threshold would speculate on a site that misses nine times
    /// in ten.
    #[test]
    fn overflow_keeps_the_total_honest() {
        let mut observation = Observation::new();
        for at in 0..WITNESS_WIDTH {
            observation.saw(&Witness::of(&format!("kept{at}")));
        }
        for at in 0..96 {
            observation.saw(&Witness::of(&format!("dropped{at}")));
        }

        assert!(observation.overflowed());
        assert_eq!(observation.total(), (WITNESS_WIDTH + 96) as u64);
        let majority = observation.majority().expect("a witness was kept");
        assert_eq!(majority.count, 1);
        assert_eq!(majority.total, (WITNESS_WIDTH + 96) as u64);
    }

    /// A monomorphic site reports the one thing it saw, every time it saw it.
    #[test]
    fn a_site_that_saw_one_thing_says_so() {
        let mut recorder = Recorder::new();
        let shape = Witness::of_all(["x", "y"]);
        for _ in 0..1000 {
            recorder.saw(&shape);
        }
        let observation = recorder.finish();

        let majority = observation.majority().expect("reached a thousand times");
        assert_eq!(majority.witness, &shape);
        assert_eq!(majority.count, 1000);
        assert_eq!(majority.total, 1000);
        assert!(!majority.overflowed);
    }

    /// A site nothing reached has no majority, rather than an empty one.
    #[test]
    fn an_unreached_site_has_no_majority() {
        assert!(Observation::new().majority().is_none());
    }

    /// Ranking is by count, and ties are broken deterministically.
    #[test]
    fn two_runs_rank_the_same_witnesses_the_same_way() {
        let (first, second) = (Witness::of("a"), Witness::of("b"));
        let mut forwards = Observation::new();
        forwards.saw_many(&first, 7);
        forwards.saw_many(&second, 7);
        let mut backwards = Observation::new();
        backwards.saw_many(&second, 7);
        backwards.saw_many(&first, 7);

        assert_eq!(
            forwards.majority().unwrap().witness,
            backwards.majority().unwrap().witness,
            "a tie was broken by insertion order, so two runs over one program \
             would disagree about which site to speculate on"
        );
    }

    /// Two records of one site add up, and overflow does not un-happen.
    #[test]
    fn absorbing_keeps_both_the_counts_and_the_overflow() {
        let witness = Witness::of("shared");
        let mut left = Observation::new();
        left.saw_many(&witness, 10);

        let mut right = Observation::new();
        right.saw_many(&witness, 5);
        for at in 0..WITNESS_WIDTH + 1 {
            right.saw(&Witness::of(&format!("other{at}")));
        }

        left.absorb(&right);
        assert!(left.overflowed(), "overflow was lost by merging");
        assert_eq!(left.total(), 10 + 5 + WITNESS_WIDTH as u64 + 1);
        let majority = left.majority().expect("the shared witness survived");
        assert_eq!(majority.witness, &witness);
        assert_eq!(majority.count, 15);
    }
}
