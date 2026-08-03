//! Faults, and where in the program they happened.
//!
//! A trap that cannot say where it came from is a crash with a hexadecimal
//! address attached, and the only people who can act on one are the people who
//! already know the answer. The correspondence between emitted code and the
//! program it came from exists only during lowering, so if it is not recorded
//! there it does not exist at all.
//!
//! # What a position is, and deliberately is not
//!
//! A number the client gave us. This layer never interprets it, never orders it,
//! never renders it. A line number, a byte offset, an index into the client's own
//! table — all of them work, because none of them mean anything here.
//!
//! That is not squeamishness about scope. A position this layer understood would
//! be a position it could be wrong about: a line number is wrong after a macro
//! expands, a byte offset is wrong after a file is preprocessed, and the client
//! is the only thing that knows which. Carrying an opaque number is the only way
//! to be certain we are not lying about where something happened.

mod table;

pub use table::{Fault, FaultKind, FaultTable};

/// Where in the client's program something is.
///
/// Opaque by design. See the module documentation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, Default)]
pub struct Position(pub u32);

impl Position {
    /// The position that says nothing.
    ///
    /// Not an error: plenty of emitted code belongs to no single place in the
    /// program — a dispatch the frame transformation built, a cleanup copied
    /// into three paths. Claiming one of those came from somewhere in
    /// particular would be worse than admitting it came from nowhere.
    pub const UNKNOWN: Position = Position(0);

    /// Whether this says anything.
    pub fn is_known(self) -> bool {
        self != Position::UNKNOWN
    }

    /// The number the client gave, back again.
    pub fn raw(self) -> u32 {
        self.0
    }

    /// Reads a position back out of what the code generator recorded.
    ///
    /// Two things mean "nothing said", and both have to become one here. Ours is
    /// zero; the code generator's own marker for an unset location is all ones.
    /// Letting the second through would report a position no client ever gave —
    /// a very confident answer to a question nobody asked, which is worse than
    /// no answer at all.
    pub fn from_recorded(bits: u32) -> Self {
        if bits == u32::MAX {
            Position::UNKNOWN
        } else {
            Position(bits)
        }
    }
}
