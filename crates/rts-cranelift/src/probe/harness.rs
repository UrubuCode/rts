//! Running a fixture and timing it.

use std::time::Instant;

use super::fixtures::Fixture;

/// Which build produced a number.
///
/// Reported with every measurement rather than assumed, because the difference
/// between the two is large enough to reverse a conclusion, and a number without
/// it invites exactly that mistake.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Profile {
    /// Unoptimized. Not a number, whatever it says.
    Debug,
    /// Optimized.
    Release,
}

impl Profile {
    /// Which build this is.
    pub fn current() -> Self {
        if cfg!(debug_assertions) {
            Profile::Debug
        } else {
            Profile::Release
        }
    }

    /// Whether a number from this build means anything.
    pub fn is_meaningful(self) -> bool {
        self == Profile::Release
    }
}

/// What one fixture cost.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Measurement {
    /// What was measured.
    pub name: &'static str,
    /// How many times it ran.
    pub iterations: u64,
    /// How long that took, in nanoseconds.
    pub total_nanos: u128,
    /// Which build produced it.
    pub profile: Profile,
    /// What the fixture computed, kept so that nothing can be optimized away.
    ///
    /// Reported rather than discarded: a value nobody looks at is a value an
    /// optimizer may decide not to compute, and then the number measures an
    /// empty loop.
    pub checksum: i64,
}

impl Measurement {
    /// Nanoseconds per iteration.
    pub fn nanos_per_op(&self) -> f64 {
        self.total_nanos as f64 / self.iterations.max(1) as f64
    }
}

/// Compiles a fixture and times it.
///
/// `iterations` is how many times the PRIMITIVE runs, and the fixture performs
/// all of them inside one compiled call — see `fixtures.rs` for why. The
/// harness therefore calls once, which is the whole point: the indirect call
/// priced below is paid once per measurement rather than once per operation.
///
/// The compiled code is leaked deliberately: it has to outlive the pointer that
/// calls it, and a probe that freed it while measuring would be measuring
/// something else entirely.
pub fn measure(fixture: &Fixture, iterations: u64) -> Measurement {
    let compiled = fixture.compile();

    // # What the indirect call costs, and why it no longer matters
    //
    // The pointer this measurement goes through is not free. Measured
    // 2026-08-20, release, 50 million iterations, best of five:
    //
    //   the loop alone ......................... 0 ps
    //   + one indirect call .................. 1266 ps
    //   + a second one ....................... 2319 ps
    //   + an indexed buffer instead .......... 1276 ps
    //
    // **1.27 ns, against primitives that cost a fraction of one.** For as long
    // as the harness called the fixture once per operation, that was the number
    // — every row landed within noise of every other, and `field_read` even
    // read below `arithmetic`, which is impossible. `fixtures.rs` carries that
    // table and the argument.
    //
    // It is paid ONCE now, because the fixture performs `iterations` of its
    // primitive inside one call. At a million trips the call contributes about
    // a femtosecond per operation, which is what "amortised" means and what
    // subtracting a floor could never have achieved: a subtraction removes an
    // offset, and the problem was resolution.
    //
    // The ring of pre-computed arguments went with it. It existed so that
    // choosing an argument was a load rather than a second indirect call; there
    // is one argument now, and it is the trip count.

    // One run before timing, so that the first-call cost of touching fresh
    // pages and of warming the branch predictor is not attributed to the thing
    // being measured. Short, because it is a warm-up and not a measurement.
    let warmup = (iterations / 100).clamp(1, 10_000);
    let mut checksum = compiled.call(warmup as i64);

    let start = Instant::now();
    checksum = checksum.wrapping_add(compiled.call(iterations as i64));
    let elapsed = start.elapsed();

    Measurement {
        name: fixture.name,
        iterations,
        total_nanos: elapsed.as_nanos(),
        profile: Profile::current(),
        checksum,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debug_number_says_it_is_not_one() {
        // Whichever build the tests run in, the answer is stated rather than
        // guessed — which is the only property worth asserting here.
        assert_eq!(
            Profile::current().is_meaningful(),
            !cfg!(debug_assertions),
            "a number that does not say which build made it invites the wrong conclusion"
        );
    }
}
