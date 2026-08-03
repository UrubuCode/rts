//! Which entry points exist, numbered.
//!
//! # Why a list in source and not a generated table
//!
//! `rts-symbol-baker` scans for declarations and bakes a table of thousands.
//! That mechanism exists because the old runtime *has* thousands, and its own
//! documentation is clear about when it is the right one and when it is not:
//!
//! > At that size, an explicitly numbered list in source is the right
//! > mechanism, and the same list at several hundred entries would not be —
//! > which is exactly the distinction that made a generated table necessary
//! > elsewhere. A closed set a reviewer can read in one screen is not the
//! > failure mode that motivated generation; an open-ended one is.
//!
//! This is the small side of that distinction, for the same reason
//! [`rts_cranelift::symbols::RtEntry`] is: membership is decided by a rule that
//! keeps the list short. An operation is here only if it touches the heap, the
//! operating system, or global mutable state — everything else is instructions.
//!
//! # Why not `#[rtse::abi]`
//!
//! It emits an `rts_abi::SymbolDesc`, and `rts-abi` is the interface
//! `rts-cranelift::abi` replaced. Its own module documentation says why it was
//! rebuilt rather than extended: *"entirely scalar: no aggregate, no structure,
//! a return position holding zero or one machine slot, and a string that cannot
//! be returned at all… It is not a foundation."*
//!
//! Declaring a new crate through it would tie the new engine to the one being
//! removed, and route these calls through a name when the decision was to reach
//! them by index. Both are backwards.
//!
//! # The numbers are the linkage, so they are facts about the list
//!
//! Written out rather than derived from order, so a reader comparing two
//! versions can see that an entry kept its place. Adding one appends; removing
//! one leaves a gap rather than renumbering, because a caller compiled against
//! an older list would otherwise call a different function with the same number
//! and never find out.

use rts_cranelift::abi::{AbiType, Convention, Signature};
use rts_cranelift::repr::Repr;

/// One JavaScript value, as it crosses the boundary.
///
/// `Repr::Tagged` is the machine's own word for "a value nothing has proved
/// anything about", which is exactly what a `Value` is. Spelling it `Int64`
/// would be describing the register rather than the meaning, and the register is
/// the one thing both sides already agree on.
const VALUE: AbiType = AbiType::Scalar(Repr::Tagged);

/// An operation compiled code performs by calling rather than by emitting.
///
/// Numbered explicitly. The number is what a call site holds — see the module
/// documentation for why it is written rather than counted.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum CoreEntry {
    /// `a + b`, on operands already reduced to primitives.
    ///
    /// Here because joining two strings allocates. Two numbers added is
    /// arithmetic and a lowering that proved both operands are numbers should
    /// emit it rather than call this.
    Add = 0,

    /// `a === b`.
    ///
    /// Here because two strings are equal when their *text* is, which needs the
    /// heap. Everything else about it is a comparison.
    StrictEquals = 1,

    /// `ToBoolean`.
    ///
    /// Here for one falsy case out of seven: the empty string. A lowering that
    /// proved its operand is a number should emit the comparison.
    ToBoolean = 2,

    /// `String(n)`.
    ///
    /// Here because the result is allocated.
    NumberToString = 3,
}

/// How many entry points exist.
///
/// One past the last number, not a count of variants: a removed entry leaves its
/// number unused, and a dense array keyed by the number must still have room for
/// it.
pub const CORE_ENTRY_COUNT: usize = 4;

impl CoreEntry {
    /// Every entry, in numbered order.
    pub const ALL: &'static [CoreEntry] = &[
        CoreEntry::Add,
        CoreEntry::StrictEquals,
        CoreEntry::ToBoolean,
        CoreEntry::NumberToString,
    ];

    /// The number a call site holds.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The linker name, for the object file and for a backtrace.
    ///
    /// A name is still needed in two places and neither is the call site: an
    /// object file resolves an undefined symbol against the archive by name, and
    /// a backtrace naming `__rts_add` is readable where one naming index 0 is
    /// not. Keeping the name as *description* rather than as the mechanism is
    /// the whole distinction.
    pub fn symbol(self) -> &'static str {
        match self {
            CoreEntry::Add => "__rts_add",
            CoreEntry::StrictEquals => "__rts_strict_equals",
            CoreEntry::ToBoolean => "__rts_to_boolean",
            CoreEntry::NumberToString => "__rts_number_to_string",
        }
    }

    /// What it accepts and returns.
    ///
    /// Built with the machine's own [`Signature`], not a second description of
    /// one. The compiler emitting a call and the runtime defining it read the
    /// same value, so a mismatch is a compile error rather than a wrong number
    /// of arguments discovered at run time.
    pub fn signature(self) -> Signature {
        match self {
            CoreEntry::Add => Signature::foreign(vec![VALUE, VALUE], vec![VALUE]),
            CoreEntry::StrictEquals => {
                Signature::foreign(vec![VALUE, VALUE], vec![AbiType::Scalar(Repr::Bool)])
            }
            CoreEntry::ToBoolean => {
                Signature::foreign(vec![VALUE], vec![AbiType::Scalar(Repr::Bool)])
            }
            CoreEntry::NumberToString => {
                Signature::foreign(vec![AbiType::Scalar(Repr::F64)], vec![VALUE])
            }
        }
    }

    /// Which convention it uses.
    ///
    /// Foreign, every one: these are `extern "C"` definitions the linker
    /// resolves, so their convention is the target's and not ours to choose.
    pub fn convention(self) -> Convention {
        Convention::Foreign
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numbers_are_written_and_dense() {
        for (position, entry) in CoreEntry::ALL.iter().enumerate() {
            assert_eq!(
                entry.index(),
                position,
                "{entry:?} sits where its number says — a list whose numbers \
                 came from its order would not survive a removal"
            );
        }
        assert_eq!(CoreEntry::ALL.len(), CORE_ENTRY_COUNT);
    }

    #[test]
    fn every_entry_has_its_own_name() {
        let mut names: Vec<&str> = CoreEntry::ALL.iter().map(|e| e.symbol()).collect();
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "two entries sharing a name would link");
    }

    #[test]
    fn a_signature_says_what_the_definition_says() {
        // Not a restatement — the definitions below take exactly these, and a
        // change to one without the other stops compiling.
        assert_eq!(CoreEntry::Add.signature().params.len(), 2);
        assert_eq!(CoreEntry::ToBoolean.signature().params.len(), 1);
        assert_eq!(
            CoreEntry::NumberToString.signature().params,
            vec![AbiType::Scalar(Repr::F64)],
            "a number goes in as a number, not as a tagged value — the caller already proved it"
        );
    }

    #[test]
    fn the_list_is_short_enough_to_read_in_one_screen() {
        // The membership rule's whole job. If this ever fails, the question is
        // not whether to raise the number — it is which of the new entries is
        // arithmetic wearing a call.
        assert!(
            CORE_ENTRY_COUNT <= 64,
            "an explicitly numbered list stops being the right mechanism when \
             nobody can read it"
        );
    }
}
