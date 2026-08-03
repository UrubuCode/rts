//! Opaque handles into a function's tables.
//!
//! Every handle is an index. None of them carry the thing they name, which is
//! deliberate: a handle that carried its own representation would have to be
//! kept in agreement with the table, and the table is the authority.

/// A value produced by an instruction or received as a block parameter.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct ValueId(pub(crate) u32);

/// A basic block.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct BlockId(pub(crate) u32);

/// An instruction within a function.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct InstId(pub(crate) u32);

/// A constant declared by a function.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub struct ConstId(pub(crate) u32);

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

index_accessor!(ValueId, BlockId, InstId, ConstId);
