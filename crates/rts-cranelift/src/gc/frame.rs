//! The frame descriptor: one record, three consumers.
//!
//! Root reporting, unwinding and suspension are not three tables. A point that
//! can collect, inside a protected region, inside a function that may suspend,
//! is *one* program point in all three concerns — three tables keyed by that
//! same point would have to be kept in agreement by hand, which is the class of
//! bug this record exists to make impossible.
//!
//! The payoff is not tidiness. A parked frame — one waiting on something,
//! occupying no position on any call stack — is still reasoned about as a frame:
//! its roots are read out of its spilled storage using these same records, and
//! its protected region is preserved, so resuming inside a handler
//! re-establishes the correct cleanup chain.
//!
//! Two of the three fields are populated by phases that do not exist yet. They
//! are declared now precisely so those phases extend this record instead of
//! introducing a second one.

use crate::frame::ResumeLabel;
use crate::ir::{InstId, ValueId};
use crate::repr::RefKind;
use crate::unwind::RegionId;

/// One value the collector must be able to find.
///
/// Named by the value it reports rather than by a register, because a register
/// has no address. Turning a value into a stack location is the spill the
/// producer arranges; until then the value is the stable name for it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RootSlot {
    /// The value that must be reported.
    pub value: ValueId,
    /// What it points at, as far as the machine is concerned.
    ///
    /// Absent for a value in the generic form: it may hold a reference and
    /// nothing at this point can prove which kind, so the collector inspects it
    /// at collection time rather than trusting a claim made here.
    pub kind: Option<RefKind>,
}

/// Everything a frame must remember at one program point.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FrameDescriptor {
    /// The instruction this record describes.
    ///
    /// Becomes a code offset once the function is emitted. Until then the
    /// instruction is the stable name, and the two-phase shape mirrors how the
    /// compiler underneath fills in its own offsets after register allocation.
    pub at: InstId,
    /// The values live here that the collector must find.
    pub roots: Vec<RootSlot>,
    /// The innermost protected region, once unwinding exists.
    pub region: Option<RegionId>,
    /// The resumption point, once suspension exists and this is one.
    pub resume_label: Option<ResumeLabel>,
}

impl FrameDescriptor {
    /// A descriptor reporting only roots.
    pub fn roots_only(at: InstId, roots: Vec<RootSlot>) -> Self {
        Self { at, roots, region: None, resume_label: None }
    }

    /// Whether anything must be traced here.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }
}

/// Every descriptor in one function, ordered by program point.
///
/// Ordered because the consumer is a lookup from a program point discovered at
/// run time — walking a stack, one return address at a time — and an ordered
/// table answers that by bisection instead of by scanning.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct FrameTable {
    descriptors: Vec<FrameDescriptor>,
}

impl FrameTable {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a descriptor.
    ///
    /// Descriptors are produced in program order, so appending keeps the table
    /// sorted without a sort. Debug builds check that rather than trusting it.
    pub fn push(&mut self, descriptor: FrameDescriptor) {
        debug_assert!(
            self.descriptors.last().is_none_or(|last| last.at < descriptor.at),
            "descriptors must be produced in program order"
        );
        self.descriptors.push(descriptor);
    }

    /// The descriptor for a program point, if that point is one.
    pub fn lookup(&self, at: InstId) -> Option<&FrameDescriptor> {
        self.descriptors
            .binary_search_by_key(&at, |d| d.at)
            .ok()
            .map(|i| &self.descriptors[i])
    }

    /// Every descriptor, in program order.
    pub fn iter(&self) -> impl Iterator<Item = &FrameDescriptor> {
        self.descriptors.iter()
    }

    /// How many program points this function describes.
    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    /// Whether the function describes no program point.
    ///
    /// A function with no descriptors is not necessarily wrong — it may simply
    /// never allocate. But a frame with no descriptor is exactly the frame a
    /// precise scan cannot read, so a collector must fall back to scanning that
    /// frame's range conservatively rather than concluding it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }
}
