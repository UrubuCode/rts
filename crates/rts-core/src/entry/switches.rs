//! The debug switches, asked once instead of at every operation.
//!
//! # Why this file exists
//!
//! Four places in this crate decided whether to print a diagnostic by reading
//! the environment, and three of them did it **on a path a running program takes
//! per operation** — a by-name property write, a cache miss, a refused chain
//! resolution. One of them, `objects::put`, even read the environment as the
//! LEFT operand of an `&&` whose right operand was a modulo, so the cheap test
//! that would have short-circuited it never got the chance.
//!
//! `std::env::var_os` is not a memory read on Windows. It re-encodes the name to
//! UTF-16, calls `GetEnvironmentVariableW`, and walks the process environment
//! block. Measured by `bench/isolated/src/bin/env_probe.rs`, release,
//! 2026-08-21, for a name that is **not set** — which is the case in every run
//! where nobody asked for the diagnostic:
//!
//! | shape | ns |
//! |---|---:|
//! | `env::var_os`, name absent | **172.2** |
//! | `env::var_os`, name present | 227.5 |
//! | `OnceLock<bool>` — this file | **0.43** |
//! | a plain `bool` | 0.24 |
//! | the `resolves % 20_000` test that should have been first | 0.77 |
//!
//! Four hundred times, for a switch nobody set.
//!
//! # Why a `OnceLock` and not a `static mut` read at startup
//!
//! Because there is no single startup. This crate is a library: a host installs
//! a context and runs a program, and a second host (`node:vm`, `node:repl`,
//! `rts napi`) may do it again in the same process. A value initialised on first
//! use is correct for all of them and needs nobody to remember to call an
//! initialiser — which is the same argument
//! `crates/rts-cranelift/src/probe/phase.rs:40` makes for `RTS_TIMING`, and this
//! is deliberately the same shape rather than a second one.
//!
//! # What it costs, stated
//!
//! **A switch set after the first read is ignored.** Setting one from inside a
//! running program — `process.env.RTS_CACHE_WHY = "1"` — used to take effect at
//! the next property write and now does not. Nothing does that, and a
//! diagnostic that turns on halfway through a run produces a log whose first
//! half is missing, which is worse than one that does not turn on. The same
//! trade is already taken for `RTS_TIMING`.
//!
//! # Why a macro rather than four functions written out
//!
//! Because the rule is *ask the environment once, remember the answer, name it*
//! — and it is one rule. Four hand-written copies is four places for the next
//! switch to be added slightly differently, which is the shape of the defect
//! this file is fixing in the first place: `cache::resolve` and `objects::put`
//! read the same `RTS_CACHE_WHY` and guarded it in opposite orders.

use std::sync::OnceLock;

/// Declares a switch: a name in the environment, a function that answers
/// whether it is set, and a `OnceLock` that makes the answer cost one load.
macro_rules! switches {
    ($($(#[$doc:meta])* $name:ident => $variable:literal),* $(,)?) => {
        $(
            $(#[$doc])*
            ///
            /// Read from the environment once, on the first call. See this
            /// module's documentation for what that costs and what it buys.
            pub(super) fn $name() -> bool {
                static ASKED: OnceLock<bool> = OnceLock::new();
                *ASKED.get_or_init(|| std::env::var_os($variable).is_some())
            }
        )*
    };
}

switches! {
    /// `RTS_CACHE_WHY` — why a cached site resolved by name, sampled.
    ///
    /// Read on every by-name property write (`objects::put`) and on every cache
    /// miss (`cache::resolve`), which is what made it the most expensive of the
    /// four.
    cache_why => "RTS_CACHE_WHY",

    /// `RTS_CACHE_DEBUG` — the first twenty cache misses, then one in 200 000.
    ///
    /// Already guarded by its counter first, so it was the one that was not
    /// costing anything. Included so that all four are stated in one place
    /// rather than three being memoised and one being left to drift back.
    cache_debug => "RTS_CACHE_DEBUG",

    /// `RTS_CHAIN_DEBUG` — which of ten ways a prototype-chain resolution
    /// declined.
    ///
    /// Read inside the `report` closure, so it runs once per refusal, and a
    /// site that refuses forever refuses on every pass.
    chain_debug => "RTS_CHAIN_DEBUG",

    /// `RTS_GC_DEBUG` — what the cycle collector found.
    gc_debug => "RTS_GC_DEBUG",
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole file is for: the answer is stable, so the cost is
    /// paid at most once however many times it is asked.
    ///
    /// Written as "twice agrees" rather than by counting the reads, because a
    /// `OnceLock` has no way to report how many times it initialised — which is
    /// the same property that makes it correct.
    #[test]
    fn a_switch_answers_the_same_thing_every_time() {
        assert_eq!(cache_why(), cache_why());
        assert_eq!(chain_debug(), chain_debug());
    }

    /// A switch nobody set is off. Names the actual default, so that a typo in
    /// the environment variable's spelling shows up as this test passing for the
    /// wrong reason rather than not at all.
    #[test]
    fn nothing_is_on_in_a_test_run() {
        assert!(!cache_why(), "RTS_CACHE_WHY is not set while testing");
        assert!(!gc_debug(), "RTS_GC_DEBUG is not set while testing");
    }
}
