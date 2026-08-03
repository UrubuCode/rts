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

    // One run before timing, so that the first-call cost of touching fresh
    // pages is not attributed to the thing being measured.
    let mut checksum = compiled.call(fixture.argument_for(0));

    let start = Instant::now();
    for step in 0..iterations {
        checksum = checksum.wrapping_add(compiled.call(fixture.argument_for(step)));
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
