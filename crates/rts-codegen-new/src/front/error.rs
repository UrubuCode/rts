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
pub struct Unsupported(pub String);

impl Unsupported {
    /// Build an `Unsupported` from anything string-like.
    pub fn new(reason: impl Into<String>) -> Self {
        Unsupported(reason.into())
    }

    /// The reason text.
    pub fn reason(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Unsupported {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unsupported in the numeric subset: {}", self.0)
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
