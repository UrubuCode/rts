//! Opaque handles into a function's tables.
//!
//! Every handle is an index. None of them carry the thing they name, which is
//! deliberate: a handle that carried its own representation would have to be
//! kept in agreement with the table, and the table is the authority.

/// A value produced by an instruction or received as a block parameter.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(pub(crate) u32);

/// A basic block.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub(crate) u32);

/// An instruction within a function.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InstId(pub(crate) u32);

/// A constant declared by a function.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstId(pub(crate) u32);

/// Somewhere a property read remembers what it last saw.
///
/// One per site rather than one per program: a site that read one kind of object
/// a thousand times and another once should remember the thousand. Sharing would
/// make every site as confused as the most confused one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CacheId(pub(crate) u32);

macro_rules! index_accessor {
    ($($ty:ty),+ $(,)?) => {
        $(impl $ty {
            /// The underlying table index.
            pub fn index(self) -> usize {
                self.0 as usize
            }
        })+
    };
}

index_accessor!(ValueId, BlockId, InstId, ConstId, CacheId);

/// `Debug` written by hand, one prefix per handle, so that a printed
/// instruction reads `IntArith(Add, v0, v1)` rather than
/// `IntArith(Add, ValueId(0), ValueId(1))`.
///
/// Derived on the handles rather than written once in the text renderer
/// (`super::text`) on purpose: the renderer prints an instruction through its
/// own `Debug`, which is what keeps it from carrying a match arm per variant
/// that a new instruction would have to remember to update. Every operand
/// inside that output is a handle, so the handles are where the naming has to
/// live for the printed form to be readable at all — and every other diagnostic
/// in the crate that prints one gets the same shortening for free.
macro_rules! debug_as {
    ($($ty:ty => $prefix:literal),+ $(,)?) => {
        $(impl core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        })+
    };
}

debug_as!(
    ValueId => "v",
    BlockId => "block",
    InstId => "inst",
    ConstId => "c",
    CacheId => "cache",
);
