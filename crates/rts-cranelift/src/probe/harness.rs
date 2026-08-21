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
/// The compiled code is leaked deliberately: it has to outlive the pointer that
/// calls it, and a probe that freed it while measuring would be measuring
/// something else entirely.
pub fn measure(fixture: &Fixture, iterations: u64) -> Measurement {
    let compiled = fixture.compile();

    // # Why the arguments are computed BEFORE the clock starts
    //
    // `Fixture::argument_for` is a call through a `fn(u64) -> i64` pointer, and
    // it used to happen inside the timed loop — so every measurement paid TWO
    // indirect calls per iteration, one for the primitive and one to decide
    // what to hand it. The second belongs to the instrument, not to anything
    // this crate wants to know the cost of.
    //
    // Measured 2026-08-20, release, 50 million iterations, best of five:
    //
    //   the loop alone ......................... 0 ps
    //   + one indirect call .................. 1266 ps
    //   + a second one, as this did .......... 2319 ps
    //   + an indexed buffer instead .......... 1276 ps
    //
    // So the second call was 1053 ps of every number here, and replacing it
    // with a load costs nothing measurable. It cancelled in the SUBTRACTION —
    // every fixture paid it, the floor included — but it did not cancel in the
    // resolution: the jitter this instrument has to see past scales with the
    // floor, and the floor was a third larger than it needed to be.
    //
    // # Why a ring and not one argument per iteration
    //
    // Fifty million `i64` is 400 MB, and a buffer that does not fit in cache
    // would put a memory stall back where the call was. A thousand entries is
    // 8 KB — L1 — and the step still varies, which is the property
    // `argument_for` exists for: its doc records a reference walking off the
    // end of the heap when every fixture was handed the raw step. Each entry
    // here came from `argument_for` on a real step, so every value passed is
    // one that function chose.
    const RING: usize = 1024;
    let arguments: Vec<i64> = (0..RING as u64).map(|step| fixture.argument_for(step)).collect();

    // One run before timing, so that the first-call cost of touching fresh
    // pages is not attributed to the thing being measured.
    let mut checksum = compiled.call(arguments[0]);

    let start = Instant::now();
    for step in 0..iterations {
        checksum = checksum.wrapping_add(compiled.call(arguments[step as usize & (RING - 1)]));
    }
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
