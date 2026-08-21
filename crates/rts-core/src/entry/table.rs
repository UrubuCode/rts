//! Which entry points exist, numbered.
//!
//! # Why a list in source and not a generated table
//!
//! `rts-symbol-baker` scans for declarations and bakes a table of thousands.
//! That mechanism exists because the old runtime *has* thousands, and its own
//! documentation is clear about when it is the right one and when it is not:
//!
//! > At that size, an explicitly numbered list in source is the right
//! > mechanism, and the same list at several hundred entries would not be â€”
//! > which is exactly the distinction that made a generated table necessary
//! > elsewhere. A closed set a reviewer can read in one screen is not the
//! > failure mode that motivated generation; an open-ended one is.
//!
//! This is the small side of that distinction, for the same reason
//! [`rts_cranelift::symbols::RtEntry`] is: membership is decided by a rule that
//! keeps the list short. An operation is here only if it touches the heap, the
//! operating system, or global mutable state â€” everything else is instructions.
//!
//! # Why not `#[rtse::abi]`
//!
//! It emits an `rts_abi::SymbolDesc`, and `rts-abi` is the interface
//! `rts-cranelift::abi` replaced. Its own module documentation says why it was
//! rebuilt rather than extended: *"entirely scalar: no aggregate, no structure,
//! a return position holding zero or one machine slot, and a string that cannot
//! be returned at allâ€¦ It is not a foundation."*
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

use super::accessor::{DEFINE_METHOD_ENTRY, DEFINE_GETTER_ENTRY, DEFINE_SETTER_ENTRY};
use super::array::{ARRAY_NEW_ENTRY, ARRAY_OF_ENTRY, ENUMERATE_KEYS_ENTRY, OWN_KEYS_ENTRY};
use super::math::MATH_RANDOM_ENTRY;
use super::text::{STRING_OF_ENTRY, TEMPLATE_JOIN_ENTRY};
use super::bitwise::{
    BIT_AND_ENTRY, BIT_NOT_ENTRY, BIT_OR_ENTRY, BIT_XOR_ENTRY, EXPONENT_ENTRY, SHIFT_LEFT_ENTRY,
    SHIFT_RIGHT_ENTRY, SHIFT_RIGHT_UNSIGNED_ENTRY,
};
use super::chain::{GET_PROTOTYPE_ENTRY, SET_PROTOTYPE_ENTRY};
use super::computed::{
    DELETE_PROPERTY_ENTRY, GET_INDEXED_ENTRY, HAS_PROPERTY_ENTRY, KEY_NUMBER_ENTRY,
    SET_INDEXED_ENTRY, WITH_HAS_ENTRY,
};
use super::functions::{
    CALL_COUNTED_ENTRY, CALL_WITH_ARGS_ENTRY, CLOSURE_NEW_ENTRY, CONSTRUCT_ENTRY,
    CONSTRUCT_WITH_ARGS_ENTRY, INSTANCE_OF_ENTRY,
    MARK_CLASS_CONSTRUCTOR_ENTRY, MARK_DERIVED_ENTRY, NEW_TARGET_ENTRY, REST_ARGUMENTS_ENTRY,
    SET_CALL_NAME_ENTRY, SUPER_CONSTRUCT_ENTRY, SUPER_CONSTRUCT_WITH_ARGS_ENTRY,
};
use super::objects::{
    GET_PROPERTY_ENTRY, GET_SUPER_PROPERTY_ENTRY, OBJECT_NEW_ENTRY, OBJECT_SPREAD_ENTRY,
    SET_PROPERTY_ENTRY, SET_SUPER_PROPERTY_ENTRY,
};
use super::function_proto::RUNNING_FUNCTION_ENTRY;
use super::generator::{DELEGATE_STEP_ENTRY, GENERATOR_NEW_ENTRY, GENERATOR_YIELD_ENTRY};
use super::arguments::ARGUMENTS_OBJECT_ENTRY;
use super::eval_scope::EVAL_DIRECT_ENTRY;
use super::promise::ASYNC_START_ENTRY;
use super::dynamic_module::{IMPORT_META_ENTRY, MODULE_IMPORT_ENTRY};
use super::modules::MODULE_PUBLISH_ALL_ENTRY;
use super::array::{ELEMENTS_BASE_ENTRY, ELEMENT_AT_ENTRY};
use super::throw::{TAKE_THROWN_ENTRY, THROWN_ADDRESS_ENTRY, THROWN_ENTRY};
use super::operators::LOOSE_EQUALS_ENTRY;
use super::operators::{
    DIVIDE_ENTRY, GREATER_ENTRY, GREATER_EQUAL_ENTRY, LESS_ENTRY, LESS_EQUAL_ENTRY, MULTIPLY_ENTRY,
    NUMBER_REMAINDER_ENTRY, REMAINDER_ENTRY, SUBTRACT_ENTRY,
};
use super::primitives::{ADD_ENTRY, NUMBER_TO_STRING_ENTRY, STRICT_EQUALS_ENTRY, TO_BOOLEAN_ENTRY};
use super::global::{
    GLOBAL_GET_ENTRY, GLOBAL_GET_UNBOUND_ENTRY, GLOBAL_SET_ENTRY, SLOPPY_THIS_ENTRY,
};
use super::iterate::{ARRAY_APPEND_ALL_ENTRY, ARRAY_APPEND_ENTRY, ITERATE_ENTRY};
use super::bigint_class::{BIGINT_NEW_ENTRY, NEGATE_ENTRY};
use super::regex::REGEX_NEW_ENTRY;
use super::modules::{MODULE_BINDING_ENTRY, MODULE_NAMESPACE_ENTRY, MODULE_PUBLISH_ENTRY};
use super::text::{STRING_CONST_ENTRY, TEMPLATE_STRINGS_ENTRY, TYPE_OF_ENTRY};

/// An operation compiled code performs by calling rather than by emitting.
///
/// Numbered explicitly. The number is what a call site holds â€” see the module
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
    ///
    /// Unlike its four neighbours, the operation itself is **not** one
    /// instruction — the sentence claiming so stood here and was false. A pass
    /// that proves both operands are numbers emits a call to
    /// [`NumberRemainder`](CoreEntry::NumberRemainder), not an instruction; see
    /// that row for why the machine cannot express this one exactly.
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

    /// `{}` â€” a new object.
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
    /// Here because finding out whether a value is code reads the heap â€” and
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

    /// `o[e]` â€” read a property the program computed the name of.
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

    /// `[â€¦]` â€” a new array.
    ///
    /// Here because it allocates, and because elements live in a store the
    /// region does not hold.
    ArrayNew = 31,

    /// `delete o.x`.
    ///
    /// Here because removing a property rebuilds the layout and moves what is
    /// left, both of which touch the heap.
    DeleteProperty = 32,

    /// `for (k in o)` â€” the keys, as an array of strings.
    ///
    /// Here because it walks a layout and allocates the array it answers with.
    OwnKeys = 33,

    /// `new f(â€¦)`.
    ///
    /// Here because it allocates and links a prototype before calling.
    Construct = 34,

    /// `v instanceof f`.
    ///
    /// Here because it walks a prototype chain through the heap.
    InstanceOf = 35,

    /// `/pattern/flags` â€” a new regular expression object.
    ///
    /// Here because it allocates, and because compiling a pattern is state this
    /// crate holds beside the cell. Not a constant the compiler could emit: a
    /// literal evaluated twice is two objects with their own `lastIndex`.
    RegexNew = 36,

    /// The value a name the runtime provides has.
    ///
    /// Here because those values are allocated, and because they are held as
    /// properties of an object this crate owns.
    GlobalGet = 37,

    /// What an object inherits from.
    ///
    /// Here because it reads state beside the cell. A class definition is
    /// lowered into this and its partner, which is why they exist before any
    /// `Object` method a program could call does.
    GetPrototype = 38,

    /// Links an object to what it inherits from.
    SetPrototype = 39,

    /// Writes a global, creating it.
    ///
    /// Here for the reason the read is: the values live on an object this
    /// crate owns.
    GlobalSet = 40,

    /// `get x() { â€¦ }` â€” records the getter half of an accessor.
    ///
    /// Here because it writes state beside the cell. Deliberately not a shape
    /// transition: a getter in the layout would be RETURNED by the cache
    /// instead of called.
    DefineGetter = 41,

    /// `set x(v) { â€¦ }` â€” the setter half.
    DefineSetter = 42,

    /// `super(â€¦)` â€” the parent constructor, producing the object.
    ///
    /// Here because only the base of a chain knows what kind of object to
    /// allocate, and because the class `new` named has to survive every
    /// `super()` between the two.
    SuperConstruct = 43,

    /// Records that a constructor must ask its parent for the object.
    MarkDerived = 44,

    /// Calling with more arguments than the convention carries.
    ///
    /// Here because the vector is allocated and because where the arguments of
    /// a running call live is this crate.s question to answer.
    CallWithArgs = 45,

    /// `...rest` â€” the arguments past the declared ones.
    RestArguments = 46,

    /// `new f(â€¦)` with more arguments than the convention carries.
    ConstructWithArgs = 47,

    /// The elements of an iterable, as an array.
    ///
    /// Here because it allocates, and because what a value yields is this
    /// crate.s question: `for-of` becomes the indexed loop `for-in` already is.
    Iterate = 48,

    /// Appends one value to an array.
    ArrayAppend = 49,

    /// Appends everything an iterable yields.
    ArrayAppendAll = 50,

    /// The bigint a literal names, from its digits as an interned string.
    BigIntNew = 51,

    /// `-x`, which a bigint cannot reach through a multiply.
    Negate = 52,

    /// The strings object of a tagged-template site, by its number.
    ///
    /// Here because the site has ONE object for the life of the program, and the
    /// only thing that outlives an activation is this crate.
    TemplateStrings = 53,

    /// One name imported from a module the host provided.
    ///
    /// Here because the namespace is an object in the heap, and which specifier
    /// is a literal this crate holds the text of.
    ModuleBinding = 54,

    /// The whole namespace object, for `import * as ns`.
    ModuleNamespace = 55,
    /// One exported binding written into the specifier table â€” the write whose
    /// read is [`CoreEntry::ModuleBinding`].
    ModulePublish = 56,

    /// Copies a source object's own enumerable properties onto a target.
    ObjectSpread = 59,

    /// The key number a value resolves to.
    KeyNumber = 60,

    /// Whether a throw is in flight.
    ///
    /// Numbered after everything that existed, because a number is never reused
    /// and never renumbered: compiled code names an entry by its position.
    Thrown = 57,

    /// The value in flight, clearing it.
    TakeThrown = 58,

    /// [`super::running_function`].
    RunningFunction = 61,

    /// [`super::generator_new`].
    GeneratorNew = 62,

    /// [`super::generator_yield`].
    GeneratorYield = 63,

    /// [`super::module_publish_all`].
    ModulePublishAll = 64,

    /// Records that a callable is a class constructor, reachable only through
    /// `new`.
    ///
    /// Here for the reason [`CoreEntry::MarkDerived`] is: it writes state
    /// beside the cell.
    MarkClassConstructor = 65,

    /// [`super::set_call_name`].
    SetCallName = 66,

    /// [`super::get_super_property`].
    GetSuperProperty = 67,

    /// [`super::set_super_property`].
    SetSuperProperty = 68,

    /// `super(...args)`, or `super(â€¦)` with more than four arguments â€” the
    /// parent constructor, over an arbitrary-length vector.
    ///
    /// Not `ConstructWithArgs`: that entry SETS `new.target`, and `super()`
    /// must not. Vector-shaped like it, `new.target`-inert like
    /// [`CoreEntry::SuperConstruct`] â€” nothing already numbered was both.
    SuperConstructWithArgs = 69,

    /// A read of a name proved, while compiling, to be neither declared,
    /// provided, nor created by a sloppy write.
    ///
    /// Here because it raises the pending throw [`super::throw::
    /// reference_error`] holds, which is global mutable state â€” the same
    /// reason [`CoreEntry::GlobalGet`] and [`CoreEntry::GlobalSet`] are.
    GlobalGetUnbound = 70,

    /// `[a, b, c, d]` â€” the array and every element, in one crossing.
    ArrayOf = 71,

    /// `Math.random()`, reached without a property read or a dispatch.
    MathRandom = 72,

    /// A template literal, joined in one crossing.
    TemplateJoin = 73,

    /// Where this thread's throw flag lives, so a check is a load.
    ThrownAddress = 74,

    /// An element of a proven array at a proven index.
    ElementAt = 75,

    /// Where a proven array's elements start, as an address.
    ElementsBase = 76,

    /// `ToString(value)` â€” the conversion with the STRING hint.
    ///
    /// Here rather than lowered as `+ ""` because the hint is the difference:
    /// `+` converts with the DEFAULT hint, which asks `valueOf` first, and a
    /// template substitution asks `toString` first. No spelling of the operator
    /// answers the second question.
    StringOf = 77,

    /// The keys a `for`-`in` visits: enumerable, along the PROTOTYPE CHAIN.
    ///
    /// Here for the reason [`CoreEntry::OwnKeys`] is â€” it walks layouts and
    /// allocates the array it answers with â€” and SEPARATE from it because the
    /// two answer different questions that are both asked. Object rest is
    /// own-only by specification; `for`-`in` walks up, and walked only the own
    /// keys until this existed.
    EnumerateKeys = 78,

    /// `class C { m() {} }` â€” a member of a class, written NON-ENUMERABLE.
    ///
    /// Separate from a property write because the attribute is the whole
    /// difference, and a write carries none. An instance FIELD stays an
    /// ordinary write: the language makes that one enumerable.
    DefineMethod = 79,

    /// One turn of a `yield*`: [`super::delegate_step`].
    ///
    /// Here rather than emitted, although the call it makes is an ordinary one
    /// the emitter could write: it also records which iterator the generator
    /// being resumed is delegating to, which is global mutable state â€” the
    /// membership rule's third clause â€” and is the only way `g.throw(e)` can
    /// reach the inner iterator's own `throw`.
    DelegateStep = 80,

    /// `new.target` â€” the constructor `new` named, for the activation asking.
    ///
    /// The membership rule's THIRD clause and not the heap one: the answer is a
    /// read of the target stack, which is global mutable state, and no
    /// instruction can see it. The calling convention carries no "was this a
    /// construct" bit â€” inventing one would be a machine change every call in
    /// the program pays for, so that a meta-property almost no program writes
    /// could be read without a call.
    NewTarget = 81,

    /// `import.meta` â€” [`super::import_meta`].
    ///
    /// The heap clause and the identity one together: the object is built once
    /// per module and kept for the life of the program, and only this crate
    /// outlives an activation. An object literal emitted at each occurrence
    /// would make `import.meta === import.meta` false.
    ImportMeta = 82,

    /// `import(specifier)` â€” [`super::module_import`].
    ///
    /// Allocates a promise and reads the specifier table: the first clause
    /// twice over. Distinct from [`CoreEntry::ModuleNamespace`] because the
    /// specifier is a VALUE the program computed rather than a literal the
    /// compiler resolved, which is the whole reason a dynamic import exists.
    ModuleImport = 83,

    /// The receiver a NON-STRICT function was entered with â€” [`super::sloppy_this`].
    ///
    /// The heap clause: the substitute is the global object, which this crate
    /// makes on demand and holds. Reached only from a body the compiler knows
    /// is non-strict, which in this engine means text `Function`/`eval`
    /// compiled into a program that is already running.
    SloppyThis = 84,

    /// The promise a call to an `async function` answers â€” [`super::async_start`].
    ///
    /// The heap clause twice over: it allocates the body's frame and the
    /// promise. What it does that no other entry does is DRIVE the frame, which
    /// is the third clause â€” the promise reaction that resumes it is the
    /// runtime's own table, and no instruction can attach one.
    AsyncStart = 85,

    /// `arguments` â€” the array-LIKE object, not the array
    /// [`RestArguments`](CoreEntry::RestArguments) builds.
    ///
    /// Here for the same reason that one is â€” the object is allocated, and
    /// where the arguments of a running call live is this crate's question â€”
    /// and separate from it because what comes out differs in a way a program
    /// reads: `Array.isArray(arguments)` is `false`.
    ArgumentsObject = 86,

    /// [`super::eval_direct`] â€” a call whose callee was written as the bare
    /// name `eval`, with the caller's environment beside the source.
    ///
    /// A row rather than an ordinary call because direct and indirect `eval`
    /// are the SAME value called two ways: only the emitter can tell them
    /// apart, and only a distinct entry can carry the environment that
    /// difference is about.
    EvalDirect = 87,

    /// [`super::with_has`] â€” whether a `with` scope resolves a name against its
    /// object, which is `in` minus what `Symbol.unscopables` blocks.
    ///
    /// A row rather than the emitter calling [`HasProperty`](CoreEntry::
    /// HasProperty) and reading the list itself, because reading the list is a
    /// property read through a prototype chain and the answer decides which
    /// BINDING a name means â€” asking it in two instructions would put the
    /// unscopables rule in the language layer, where a second spelling of it
    /// would eventually disagree with this one.
    WithHas = 88,

    /// [`super::number_remainder`] â€” `a % b` over two proven doubles.
    ///
    /// The one row here that is NOT justified by touching the heap, the
    /// operating system or global mutable state. It is pure computation, and
    /// by the membership rule that means it should be an instruction.
    ///
    /// It cannot be one. `crates/rts-cranelift`'s `NumOp` establishes that no
    /// exact single instruction for a double remainder exists on any target
    /// here â€” `fmod` is a library call, and the identity that would avoid it
    /// loses the low bits once the quotient passes 2^53. So the choice is not
    /// between a row and an instruction; it is between a row and leaving `%`
    /// on the generic [`Remainder`](CoreEntry::Remainder) forever.
    ///
    /// Reusing `Remainder` was REJECTED, and this is the reuse question this
    /// list exists to force. One number would then mean two shapes â€” tagged in
    /// and tagged out for one caller, unboxed both ways for the other â€” which
    /// is exactly the "one number must not mean two answers" the row above
    /// records. The shapes are the entire difference: the arithmetic is one
    /// `%` on two `f64` in both, written once and delegated to from here.
    NumberRemainder = 89,
}

/// How many entry points exist.
///
/// One past the last number, not a count of variants: a removed entry leaves its
/// number unused, and a dense array keyed by the number must still have room for
/// it.
pub const CORE_ENTRY_COUNT: usize = 90;

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
        CoreEntry::RegexNew,
        CoreEntry::GlobalGet,
        CoreEntry::GetPrototype,
        CoreEntry::SetPrototype,
        CoreEntry::GlobalSet,
        CoreEntry::DefineGetter,
        CoreEntry::DefineSetter,
        CoreEntry::SuperConstruct,
        CoreEntry::MarkDerived,
        CoreEntry::CallWithArgs,
        CoreEntry::RestArguments,
        CoreEntry::ConstructWithArgs,
        CoreEntry::Iterate,
        CoreEntry::ArrayAppend,
        CoreEntry::ArrayAppendAll,
        CoreEntry::BigIntNew,
        CoreEntry::Negate,
        CoreEntry::TemplateStrings,
        CoreEntry::ModuleBinding,
        CoreEntry::ModuleNamespace,
        CoreEntry::ModulePublish,
        CoreEntry::Thrown,
        CoreEntry::TakeThrown,
        CoreEntry::ObjectSpread,
        CoreEntry::KeyNumber,
        CoreEntry::RunningFunction,
        CoreEntry::GeneratorNew,
        CoreEntry::GeneratorYield,
        CoreEntry::ModulePublishAll,
        CoreEntry::MarkClassConstructor,
        CoreEntry::SetCallName,
        CoreEntry::GetSuperProperty,
        CoreEntry::SetSuperProperty,
        CoreEntry::SuperConstructWithArgs,
        CoreEntry::GlobalGetUnbound,
        CoreEntry::ArrayOf,
        CoreEntry::MathRandom,
        CoreEntry::TemplateJoin,
        CoreEntry::ThrownAddress,
        CoreEntry::ElementAt,
        CoreEntry::ElementsBase,
        CoreEntry::StringOf,
        CoreEntry::EnumerateKeys,
        CoreEntry::DefineMethod,
        CoreEntry::DelegateStep,
        CoreEntry::NewTarget,
        CoreEntry::ImportMeta,
        CoreEntry::ModuleImport,
        CoreEntry::SloppyThis,
        CoreEntry::AsyncStart,
        CoreEntry::ArgumentsObject,
        CoreEntry::EvalDirect,
        CoreEntry::WithHas,
        CoreEntry::NumberRemainder,
    ];

    /// The number a call site holds.
    pub fn index(self) -> usize {
        self as usize
    }

    /// What the definition declared, derived from its Rust signature.
    ///
    /// Read rather than restated. Writing the shape here would put it in two
    /// places â€” this file saying "two tagged parameters" and the function saying
    /// `(u64, u64)` â€” with nothing connecting them, which is the drift the
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
            CoreEntry::ArrayOf => ARRAY_OF_ENTRY,
            CoreEntry::MathRandom => MATH_RANDOM_ENTRY,
            CoreEntry::TemplateJoin => TEMPLATE_JOIN_ENTRY,
            CoreEntry::StringOf => STRING_OF_ENTRY,
            CoreEntry::GetProperty => GET_PROPERTY_ENTRY,
            CoreEntry::SetProperty => SET_PROPERTY_ENTRY,
            CoreEntry::ClosureNew => CLOSURE_NEW_ENTRY,
            CoreEntry::Thrown => THROWN_ENTRY,
            CoreEntry::TakeThrown => TAKE_THROWN_ENTRY,
            CoreEntry::ThrownAddress => THROWN_ADDRESS_ENTRY,
            CoreEntry::ElementAt => ELEMENT_AT_ENTRY,
            CoreEntry::ElementsBase => ELEMENTS_BASE_ENTRY,
            CoreEntry::RunningFunction => RUNNING_FUNCTION_ENTRY,
            CoreEntry::GeneratorNew => GENERATOR_NEW_ENTRY,
            CoreEntry::GeneratorYield => GENERATOR_YIELD_ENTRY,
            CoreEntry::AsyncStart => ASYNC_START_ENTRY,
            CoreEntry::ArgumentsObject => ARGUMENTS_OBJECT_ENTRY,
            CoreEntry::EvalDirect => EVAL_DIRECT_ENTRY,
            CoreEntry::ModulePublishAll => MODULE_PUBLISH_ALL_ENTRY,
            CoreEntry::ObjectSpread => OBJECT_SPREAD_ENTRY,
            CoreEntry::KeyNumber => KEY_NUMBER_ENTRY,
            CoreEntry::Call => CALL_COUNTED_ENTRY,
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
            CoreEntry::WithHas => WITH_HAS_ENTRY,
            CoreEntry::NumberRemainder => NUMBER_REMAINDER_ENTRY,
            CoreEntry::ArrayNew => ARRAY_NEW_ENTRY,
            CoreEntry::DeleteProperty => DELETE_PROPERTY_ENTRY,
            CoreEntry::OwnKeys => OWN_KEYS_ENTRY,
            CoreEntry::EnumerateKeys => ENUMERATE_KEYS_ENTRY,
            CoreEntry::DefineMethod => DEFINE_METHOD_ENTRY,
            CoreEntry::Construct => CONSTRUCT_ENTRY,
            CoreEntry::InstanceOf => INSTANCE_OF_ENTRY,
            CoreEntry::RegexNew => REGEX_NEW_ENTRY,
            CoreEntry::BigIntNew => BIGINT_NEW_ENTRY,
            CoreEntry::Negate => NEGATE_ENTRY,
            CoreEntry::TemplateStrings => TEMPLATE_STRINGS_ENTRY,
            CoreEntry::ModuleBinding => MODULE_BINDING_ENTRY,
            CoreEntry::ModuleNamespace => MODULE_NAMESPACE_ENTRY,
            CoreEntry::ModulePublish => MODULE_PUBLISH_ENTRY,
            CoreEntry::GlobalGet => GLOBAL_GET_ENTRY,
            CoreEntry::GlobalGetUnbound => GLOBAL_GET_UNBOUND_ENTRY,
            CoreEntry::SloppyThis => SLOPPY_THIS_ENTRY,
            CoreEntry::GetPrototype => GET_PROTOTYPE_ENTRY,
            CoreEntry::SetPrototype => SET_PROTOTYPE_ENTRY,
            CoreEntry::GlobalSet => GLOBAL_SET_ENTRY,
            CoreEntry::DefineGetter => DEFINE_GETTER_ENTRY,
            CoreEntry::DefineSetter => DEFINE_SETTER_ENTRY,
            CoreEntry::SuperConstruct => SUPER_CONSTRUCT_ENTRY,
            CoreEntry::NewTarget => NEW_TARGET_ENTRY,
            CoreEntry::ImportMeta => IMPORT_META_ENTRY,
            CoreEntry::ModuleImport => MODULE_IMPORT_ENTRY,
            CoreEntry::MarkDerived => MARK_DERIVED_ENTRY,
            CoreEntry::MarkClassConstructor => MARK_CLASS_CONSTRUCTOR_ENTRY,
            CoreEntry::SetCallName => SET_CALL_NAME_ENTRY,
            CoreEntry::GetSuperProperty => GET_SUPER_PROPERTY_ENTRY,
            CoreEntry::SetSuperProperty => SET_SUPER_PROPERTY_ENTRY,
            CoreEntry::CallWithArgs => CALL_WITH_ARGS_ENTRY,
            CoreEntry::RestArguments => REST_ARGUMENTS_ENTRY,
            CoreEntry::ConstructWithArgs => CONSTRUCT_WITH_ARGS_ENTRY,
            CoreEntry::Iterate => ITERATE_ENTRY,
            CoreEntry::ArrayAppend => ARRAY_APPEND_ENTRY,
            CoreEntry::ArrayAppendAll => ARRAY_APPEND_ALL_ENTRY,
            CoreEntry::SuperConstructWithArgs => SUPER_CONSTRUCT_WITH_ARGS_ENTRY,
            CoreEntry::DelegateStep => DELEGATE_STEP_ENTRY,
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
                "{entry:?} sits where its number says â€” a list whose numbers \
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
        // Not a restatement â€” the definitions below take exactly these, and a
        // change to one without the other stops compiling.
        use rts_cranelift::abi::AbiType;
        use rts_cranelift::repr::Repr;

        assert_eq!(CoreEntry::Add.signature().params.len(), 2);
        assert_eq!(CoreEntry::ToBoolean.signature().params.len(), 1);
        assert_eq!(
            CoreEntry::NumberToString.signature().params,
            vec![AbiType::Scalar(Repr::F64)],
            "a number goes in as a number, not as a tagged value â€” derived from \n             `value: f64`, not written here"
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
        // not whether to raise the number â€” it is which of the new entries is
        // arithmetic wearing a call.
        //
        // Asked and answered once, at 64. The three that crossed it are
        // `GeneratorNew` (allocates a frame), `GeneratorYield` (writes context
        // state) and `ModulePublishAll` (walks one namespace and writes
        // another) â€” every one of them touches the heap or global mutable
        // state, so none is arithmetic and the ceiling was what had to move.
        // It moved ONCE and by a little: the next entry to cross it has to
        // answer this question again rather than inherit the answer.
        // Moved to 74 on 2026-08-13 for `ObjectPair` and `ArrayOf`, and the
        // question was answered rather than inherited: both build a heap
        // object, so neither is arithmetic, and both exist to replace THREE
        // and FIVE crossings with one â€” a list that grows to shrink the
        // number of calls is the list doing its job.
        // Moved to 75 on 2026-08-13 for `ThrownAddress`, and the question was
        // answered again rather than inherited: it hands out the address of
        // global mutable state, which is the third clause of the membership
        // rule and not the heap one. It is also the same argument as the line
        // above â€” it exists so that a check stops being a crossing at all,
        // which is the list growing to shrink the number of calls.
        // Moved to 76 on 2026-08-14 for `ElementAt`, and the question was
        // answered again: it reads the heap, so it is not arithmetic. What is
        // new is the REASON a second entry point exists for something
        // `GetIndexed` already answers â€” it exists to ask FEWER questions,
        // because the caller proved them while compiling. A list that grows so
        // that a crossing does less work is the same argument as a list that
        // grows so that there are fewer crossings.
        // Moved to 78 on 2026-08-15 for `EnumerateKeys`, and the question was
        // answered the same way: it walks a layout and allocates, so it is not
        // arithmetic. What is new is that a second entry point exists for
        // something `OwnKeys` looks like it answers â€” it does not, because
        // `for`-`in` walks the chain and object rest must not, and one
        // operation cannot be both without a flag nobody could get right.
        // Moved to 80 on 2026-08-15 for `DelegateStep`, and the answer is the
        // membership rule's THIRD clause rather than the first two: it does not
        // allocate and it does not walk the heap, but it records which iterator
        // the generator being resumed is delegating to â€” global mutable state,
        // and the only way `g.throw(e)` can reach the inner iterator's own
        // `throw`. An emitted call could make the call and could not remember.
        //
        // Moved to 82 on 2026-08-15 for `NewTarget`, and the answer is the
        // third clause again: it neither allocates nor walks the heap, it reads
        // the target stack â€” state the runtime keeps and no instruction can
        // see. The alternative was a bit in the calling convention, which is a
        // machine change every call in the program would pay for so that a
        // meta-property almost no program writes could be read without one.
        //
        // And the argument for the LIST, which the paragraph below asks the
        // next mover to make: this entry is the last one the list should absorb
        // on its present terms. Eighty-two hand-numbered rows are past what a
        // reader checks, and the NUMBER is now the only hand-written part of
        // the row â€” `#[rtse::entry]` derives the definition and the host maps
        // it. A number that three files must agree about by inspection is
        // exactly what "one source, generated views" says a generated view
        // should be answering.
        //
        // The ceiling is a reading limit rather than a capacity one, and it has
        // moved four times in one day. That is a signal about the day and not
        // about the mechanism, but the next move should come with an argument
        // for the LIST rather than for the entry.
        // Moved to 84 on 2026-08-15 for `ImportMeta` and `ModuleImport`, and
        // the entry-level question is not the hard one here: both allocate a
        // heap object â€” an `import.meta` and a promise â€” so neither is
        // arithmetic wearing a call, and both read the specifier table, which
        // is global mutable state. The membership rule admits them twice over.
        //
        // The argument for the LIST, which the paragraph above asks for and
        // which this move does NOT make: there isn't one. Two more hand-numbered
        // rows past a limit already declared unreadable is the mechanism debt
        // being paid again rather than settled, and it is recorded here as debt
        // instead of being dressed up as a reason. What makes the debt bearable
        // and not silent is that these two close the module system's last two
        // holes â€” a program could not write `import.meta` or `import()` at all
        // â€” so the alternative to the rows was a refusal by name, not a
        // different design. The generated view `#[rtse::entry]` already has
        // enough information to produce is the thing that ends this, and the
        // next mover inherits an argument that has now failed to be made twice.
        // Moved to 87 on 2026-08-15 for `ArgumentsObject`. The entry-level
        // question is easy â€” it allocates an object and reads the running
        // call's argument vector, which is global mutable state â€” and the
        // LIST-level argument this ceiling asks for is still not made. What it
        // is NOT is a row bought cheaply: the alternative was leaving
        // `arguments` as an Array, which is a wrong answer to
        // `Array.isArray` and gives every `arguments` a `map` the language
        // says it has not. Reusing `RestArguments` with a sentinel `from` was
        // rejected: one number would then mean two shapes of result, which is
        // the kind of second meaning this list exists to keep out.
        // Moved to 88 on 2026-08-16 for `WithHas`. The entry-level question is
        // easy â€” it walks a prototype chain and reads two properties, which is
        // the heap â€” and the LIST-level argument is still not made. What this
        // row is NOT is a convenience: the alternative was the emitter asking
        // `HasProperty` and then reading `Symbol.unscopables` itself, which
        // puts a scoping rule in the language layer in a second spelling, and
        // the day the two disagree a `with` resolves the wrong binding
        // silently. Reusing `HasProperty` was rejected for the same reason one
        // number must not mean two answers.
        // Moved to 89 on 2026-08-20 for `NumberRemainder`, and this is the one
        // row whose ENTRY-level question is not easy: it is pure computation,
        // which the module header says is instructions. The answer is that the
        // machine provably cannot express it exactly — `NumOp`'s documentation
        // in `rts-cranelift` carries the proof — so the rule's premise, that an
        // instruction was available, does not hold here.
        //
        // The LIST-level argument, which this ceiling exists to force: a reader
        // can still hold the list, and the alternative was not a shorter list.
        // It was `%` never leaving the generic path, which measurement on
        // 2026-08-20 put at 13 proven instructions against 16 generic calls in
        // a body whose every local is annotated `number` — because a local
        // reassigned through `%` was unprovable, which made everything
        // downstream of it unprovable too.
        //
        // Reusing `Remainder` was REJECTED: one number would mean two shapes,
        // tagged both ways for one caller and unboxed both ways for the other.
        assert!(
            CORE_ENTRY_COUNT <= 90,
            "an explicitly numbered list stops being the right mechanism when \
             nobody can read it"
        );
    }
}
