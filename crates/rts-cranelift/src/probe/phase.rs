//! Where the wall clock of one compilation went, when `RTS_TIMING` asks.
//!
//! # Why this exists
//!
//! A file in the corpus costs ~60 ms end to end and ~11 ms of that is the
//! process starting, which was measured before this module was written. The
//! remainder was unattributable: parsing, emission, placement and running are
//! one call from the outside, so every claim about which of them dominates was
//! reasoning rather than measurement — and this repository's record on
//! unmeasured performance premises is four refuted campaigns long. The first
//! thing it answered was that placement is 85% of a real file's compile time
//! and the front end is 5%, which is the reverse of where the work would have
//! gone by intuition.
//!
//! # Why it lives here and not in the host
//!
//! It was written in `rts-host`, where the first question was asked, and moved
//! the moment the second question — which part of placement — needed the same
//! helper. Two copies of a timer is two definitions of what a phase is, and the
//! numbers stop being comparable the day one of them changes its unit. This
//! crate already owns the measuring it does of itself ([`super`]), so it owns
//! this; the host reaches it through the machine, like everything else.
//!
//! # Why an environment variable and not a flag
//!
//! Every entry point into a compilation would have to grow the flag and thread
//! it through — `run`, `test`, `compile`, the two examples — to answer a
//! question none of them is about. The variable is read where the phase is, so
//! a caller that never heard of timing still reports it.
//!
//! # What it does not say
//!
//! Nothing about a phase's cost per unit of work: it reports one program's wall
//! clock, including whatever the operating system did to the process meanwhile.
//! It is an attribution instrument, and a number small enough to matter must be
//! confirmed by repetition. [`super::measure`] is what measures a primitive.

use std::time::Instant;

/// Whether `RTS_TIMING` is set.
///
/// Read once: a phase inside a per-function loop asks this thousands of times
/// in one compilation, and `var_os` allocates and walks the environment block
/// on every call — which would make the instrument a cost of the thing it
/// measures.
fn asked() -> bool {
    static ASKED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ASKED.get_or_init(|| std::env::var_os("RTS_TIMING").is_some())
}

/// A phase being timed. Reports on drop, so an early `?` still accounts for
/// the time it spent before failing — which is the case worth seeing, since a
/// compilation that fails slowly is the one nobody profiles.
pub struct Phase {
    name: &'static str,
    started: Option<Instant>,
}

impl Phase {
    /// Starts timing `name`, or does nothing at all when `RTS_TIMING` is unset.
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            started: asked().then(Instant::now),
        }
    }

    /// Reports `count` alongside the elapsed time, and consumes the phase.
    ///
    /// For a phase over a batch, where the total is uninterpretable without the
    /// number of things in it: 45 ms of machine compilation is a different
    /// finding at 12 functions than at 1200.
    pub fn over(mut self, count: usize) {
        if let Some(started) = self.started.take() {
            report(self.name, started.elapsed().as_secs_f64() * 1000.0, Some(count));
        }
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        if let Some(started) = self.started {
            report(self.name, started.elapsed().as_secs_f64() * 1000.0, None);
        }
    }
}

/// One line of the report.
///
/// stderr, because stdout is what a compiled program prints and the suite
/// compares against a fixture. Timing on stdout would fail every test that has
/// one.
fn report(name: &str, millis: f64, count: Option<usize>) {
    match count {
        Some(count) => eprintln!("rts-timing {name:<14} {millis:>8.3} ms  over {count}"),
        None => eprintln!("rts-timing {name:<14} {millis:>8.3} ms"),
    }
}
