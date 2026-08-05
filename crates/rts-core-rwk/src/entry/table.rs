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

use rts_cranelift::abi::{Convention, EntryDesc, Signature};

use super::array::{ARRAY_NEW_ENTRY, OWN_KEYS_ENTRY};
use super::bitwise::{
    BIT_AND_ENTRY, BIT_NOT_ENTRY, BIT_OR_ENTRY, BIT_XOR_ENTRY, EXPONENT_ENTRY, SHIFT_LEFT_ENTRY,
    SHIFT_RIGHT_ENTRY, SHIFT_RIGHT_UNSIGNED_ENTRY,
};
use super::computed::{
    DELETE_PROPERTY_ENTRY, GET_INDEXED_ENTRY, HAS_PROPERTY_ENTRY, SET_INDEXED_ENTRY,
};
use super::functions::{CALL_ENTRY, CLOSURE_NEW_ENTRY, CONSTRUCT_ENTRY, INSTANCE_OF_ENTRY};
use super::objects::{GET_PROPERTY_ENTRY, OBJECT_NEW_ENTRY, SET_PROPERTY_ENTRY};
use super::operators::LOOSE_EQUALS_ENTRY;
use super::operators::{
    DIVIDE_ENTRY, GREATER_ENTRY, GREATER_EQUAL_ENTRY, LESS_ENTRY, LESS_EQUAL_ENTRY, MULTIPLY_ENTRY,
    REMAINDER_ENTRY, SUBTRACT_ENTRY,
};
use super::primitives::{ADD_ENTRY, NUMBER_TO_STRING_ENTRY, STRICT_EQUALS_ENTRY, TO_BOOLEAN_ENTRY};
use super::text::{STRING_CONST_ENTRY, TYPE_OF_ENTRY};

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

    /// `a - b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    Subtract = 4,

    /// `a * b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    Multiply = 5,

    /// `a / b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    Divide = 6,

    /// `a % b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    Remainder = 7,

    /// `a < b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    Less = 8,

    /// `a <= b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    LessEqual = 9,

    /// `a > b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    Greater = 10,

    /// `a >= b`.
    ///
    /// Here because `ToNumber` of a string reads its text out of the heap.
    /// The operation itself is one instruction, and a pass that proved both
    /// operands are numbers should emit that instead of calling this.
    GreaterEqual = 11,

    /// `{}` — a new object.
    ///
    /// Here because making one allocates.
    ObjectNew = 12,

    /// `o.x`, the name given as its key number.
    ///
    /// Here because reading a property walks the heap.
    GetProperty = 13,

    /// `o.x = v`.
    ///
    /// Here because writing one may move the object to a new layout.
    SetProperty = 14,

    /// A function, as a value: its code and the environment it closed over.
    ///
    /// Here because the result is allocated.
    ClosureNew = 15,

    /// Calling a value, with a receiver and the arguments.
    ///
    /// Here because finding out whether a value is code reads the heap — and
    /// because a value that is NOT code must not be jumped to, which compiled
    /// code has no way to refuse.
    Call = 16,

    /// A string literal, by the number the compilation gave it.
    ///
    /// Here because two occurrences of one literal are the same string, which
    /// is interning, which reads a table this crate owns.
    StringConst = 17,

    /// `typeof v`.
    ///
    /// Here because the answer is a string, and a string is allocated.
    TypeOf = 18,

    /// `a == b`.
    ///
    /// Here because a string converts by reading its text.
    LooseEquals = 19,

    /// `a ** b`.
    ///
    /// Here because `ToNumber` of a string reads the heap.
    Exponent = 20,

    /// `a & b`.
    ///
    /// Here because `ToInt32` runs `ToNumber` first, and that reads the heap.
    BitAnd = 21,

    /// `a | b`.
    BitOr = 22,

    /// `a ^ b`.
    BitXor = 23,

    /// `~a`.
    BitNot = 24,

    /// `a << b`.
    ShiftLeft = 25,

    /// `a >> b`.
    ShiftRight = 26,

    /// `a >>> b`, the one whose result outgrows a signed thirty-two-bit value.
    ShiftRightUnsigned = 27,

    /// `o[e]` — read a property the program computed the name of.
    ///
    /// Here for the same reason the named read is, plus one: turning the value
    /// between the brackets into a key is `ToPropertyKey`, which interns text.
    GetIndexed = 28,

    /// `o[e] = v`.
    SetIndexed = 29,

    /// `k in o`.
    ///
    /// Asks whether the object HAS the property, which is not whether reading
    /// it yields `undefined`.
    HasProperty = 30,

    /// `[…]` — a new array.
    ///
    /// Here because it allocates, and because elements live in a store the
    /// region does not hold.
    ArrayNew = 31,

    /// `delete o.x`.
    ///
    /// Here because removing a property rebuilds the layout and moves what is
    /// left, both of which touch the heap.
    DeleteProperty = 32,

    /// `for (k in o)` — the keys, as an array of strings.
    ///
    /// Here because it walks a layout and allocates the array it answers with.
    OwnKeys = 33,

    /// `new f(…)`.
    ///
    /// Here because it allocates and links a prototype before calling.
    Construct = 34,

    /// `v instanceof f`.
    ///
    /// Here because it walks a prototype chain through the heap.
    InstanceOf = 35,
}

/// How many entry points exist.
///
/// One past the last number, not a count of variants: a removed entry leaves its
/// number unused, and a dense array keyed by the number must still have room for
/// it.
pub const CORE_ENTRY_COUNT: usize = 36;

impl CoreEntry {
    /// Every entry, in numbered order.
    pub const ALL: &'static [CoreEntry] = &[
        CoreEntry::Add,
        CoreEntry::StrictEquals,
        CoreEntry::ToBoolean,
        CoreEntry::NumberToString,
        CoreEntry::Subtract,
        CoreEntry::Multiply,
        CoreEntry::Divide,
        CoreEntry::Remainder,
        CoreEntry::Less,
        CoreEntry::LessEqual,
        CoreEntry::Greater,
        CoreEntry::GreaterEqual,
        CoreEntry::ObjectNew,
        CoreEntry::GetProperty,
        CoreEntry::SetProperty,
        CoreEntry::ClosureNew,
        CoreEntry::Call,
        CoreEntry::StringConst,
        CoreEntry::TypeOf,
        CoreEntry::LooseEquals,
        CoreEntry::Exponent,
        CoreEntry::BitAnd,
        CoreEntry::BitOr,
        CoreEntry::BitXor,
        CoreEntry::BitNot,
        CoreEntry::ShiftLeft,
        CoreEntry::ShiftRight,
        CoreEntry::ShiftRightUnsigned,
        CoreEntry::GetIndexed,
        CoreEntry::SetIndexed,
        CoreEntry::HasProperty,
        CoreEntry::ArrayNew,
        CoreEntry::DeleteProperty,
        CoreEntry::OwnKeys,
        CoreEntry::Construct,
        CoreEntry::InstanceOf,
    ];

    /// The number a call site holds.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the definition declared, derived from its Rust signature.
    ///
    /// Read rather than restated. Writing the shape here would put it in two
    /// places — this file saying "two tagged parameters" and the function saying
    /// `(u64, u64)` — with nothing connecting them, which is the drift the
    /// authoring attribute exists to make unrepresentable.
    pub fn describe(self) -> EntryDesc {
        match self {
            CoreEntry::Add => ADD_ENTRY,
            CoreEntry::StrictEquals => STRICT_EQUALS_ENTRY,
            CoreEntry::ToBoolean => TO_BOOLEAN_ENTRY,
            CoreEntry::NumberToString => NUMBER_TO_STRING_ENTRY,
            CoreEntry::Subtract => SUBTRACT_ENTRY,
            CoreEntry::Multiply => MULTIPLY_ENTRY,
            CoreEntry::Divide => DIVIDE_ENTRY,
            CoreEntry::Remainder => REMAINDER_ENTRY,
            CoreEntry::Less => LESS_ENTRY,
            CoreEntry::LessEqual => LESS_EQUAL_ENTRY,
            CoreEntry::Greater => GREATER_ENTRY,
            CoreEntry::GreaterEqual => GREATER_EQUAL_ENTRY,
            CoreEntry::ObjectNew => OBJECT_NEW_ENTRY,
            CoreEntry::GetProperty => GET_PROPERTY_ENTRY,
            CoreEntry::SetProperty => SET_PROPERTY_ENTRY,
            CoreEntry::ClosureNew => CLOSURE_NEW_ENTRY,
            CoreEntry::Call => CALL_ENTRY,
            CoreEntry::StringConst => STRING_CONST_ENTRY,
            CoreEntry::TypeOf => TYPE_OF_ENTRY,
            CoreEntry::LooseEquals => LOOSE_EQUALS_ENTRY,
            CoreEntry::Exponent => EXPONENT_ENTRY,
            CoreEntry::BitAnd => BIT_AND_ENTRY,
            CoreEntry::BitOr => BIT_OR_ENTRY,
            CoreEntry::BitXor => BIT_XOR_ENTRY,
            CoreEntry::BitNot => BIT_NOT_ENTRY,
            CoreEntry::ShiftLeft => SHIFT_LEFT_ENTRY,
            CoreEntry::ShiftRight => SHIFT_RIGHT_ENTRY,
            CoreEntry::ShiftRightUnsigned => SHIFT_RIGHT_UNSIGNED_ENTRY,
            CoreEntry::GetIndexed => GET_INDEXED_ENTRY,
            CoreEntry::SetIndexed => SET_INDEXED_ENTRY,
            CoreEntry::HasProperty => HAS_PROPERTY_ENTRY,
            CoreEntry::ArrayNew => ARRAY_NEW_ENTRY,
            CoreEntry::DeleteProperty => DELETE_PROPERTY_ENTRY,
            CoreEntry::OwnKeys => OWN_KEYS_ENTRY,
            CoreEntry::Construct => CONSTRUCT_ENTRY,
            CoreEntry::InstanceOf => INSTANCE_OF_ENTRY,
        }
    }

    /// The linker name, for the object file and for a backtrace.
    ///
    /// A name is still needed in two places and neither is the call site: an
    /// object file resolves an undefined symbol against the archive by name, and
    /// a backtrace naming `__rts_add` is readable where one naming index 0 is
    /// not. Keeping the name as *description* rather than as the mechanism is
    /// the whole distinction.
    pub fn symbol(self) -> &'static str {
        self.describe().symbol
    }

    /// What it accepts and returns.
    ///
    /// The machine's own [`Signature`], built from what the definition
    /// declared. The compiler emitting a call and the runtime defining it read
    /// one value, so a mismatch is a compile error rather than a wrong number of
    /// registers discovered at run time.
    pub fn signature(self) -> Signature {
        self.describe().signature()
    }

    /// Which convention it uses.
    ///
    /// Foreign, every one: these are `extern "C"` definitions the linker
    /// resolves, so their convention is the target's and not ours to choose.
    pub fn convention(self) -> Convention {
        self.describe().convention
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
        use rts_cranelift::abi::AbiType;
        use rts_cranelift::repr::Repr;

        assert_eq!(CoreEntry::Add.signature().params.len(), 2);
        assert_eq!(CoreEntry::ToBoolean.signature().params.len(), 1);
        assert_eq!(
            CoreEntry::NumberToString.signature().params,
            vec![AbiType::Scalar(Repr::F64)],
            "a number goes in as a number, not as a tagged value — derived from \n             `value: f64`, not written here"
        );
        assert_eq!(
            CoreEntry::Add.signature().params,
            vec![AbiType::Scalar(Repr::Tagged); 2],
            "and a `u64` parameter is a tagged value, which is what a Value is"
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
