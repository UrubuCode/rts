//! The explicit bail type for the numeric-subset lowering.
//!
//! The soundness contract of this engine (design pilar 2) is: a value is kept
//! unboxed only where the front-end PROVES it monomorphic; everywhere else the
//! representation widens or — in this increment — the construct is OUT OF SCOPE
//! and we **refuse** rather than emit a wrong value. So every unmodeled HIR
//! shape becomes an [`Unsupported`] error, never a silent miscompile. That is
//! precisely the unsoundness (`arr[0] + 5 → "05"`, a bool printing as `1`) this
//! redesign exists to eliminate.

use std::fmt;

/// An HIR construct the numeric-subset lowering does not (yet) handle. Carries a
/// human-readable reason naming the construct, so a failing test/caller sees
/// exactly what fell outside the proven fast path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported(pub String, Kind);

/// What kind of failure the message describes. Both travel in the same result
/// type (the whole front is `FrontResult`), but they are NOT the same thing and
/// must not print the same way: a `Lowering` bail means the ENGINE cannot
/// compile a construct; a `Runtime` failure means the compiled program RAN and
/// threw. Printing an uncaught `TypeError` as "unsupported in the numeric
/// subset" told the user the engine was incomplete when the engine was right
/// and the program was wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Lowering,
    Runtime,
}

impl Unsupported {
    /// Build an `Unsupported` from anything string-like.
    pub fn new(reason: impl Into<String>) -> Self {
        Unsupported(reason.into(), Kind::Lowering)
    }

    /// A failure of the RUNNING program (an uncaught throw), not of the
    /// lowering. Displays the message verbatim — no engine-limitation prefix.
    pub fn runtime(reason: impl Into<String>) -> Self {
        Unsupported(reason.into(), Kind::Runtime)
    }

    /// The reason text.
    pub fn reason(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            Kind::Lowering => write!(f, "unsupported in the numeric subset: {}", self.0),
            Kind::Runtime => write!(f, "{}", self.0),
        }
    }
}

impl std::error::Error for Unsupported {}

/// Result alias for the numeric-subset lowering.
pub type FrontResult<T> = Result<T, Unsupported>;

/// Convenience: build an `Err(Unsupported(..))` with a formatted reason.
macro_rules! unsupported {
    ($($arg:tt)*) => {
        Err($crate::front::error::Unsupported::new(format!($($arg)*)))
    };
}

pub(crate) use unsupported;
