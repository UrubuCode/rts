//! What the language asks the runtime to do.
//!
//! # Why this list is here and not in the runtime
//!
//! Because membership is a **language** judgement. `add` is an entry point
//! because joining two strings allocates; `to_boolean` is one because the empty
//! string is falsy and finding that out reads the heap. Neither sentence is
//! about a machine, and the machine agrees: it refuses to lower `Inst::Generic`
//! at all rather than deciding which symbol a generic addition should dial.
//!
//! ```text
//! Inst::Generic(..) => Err(LowerError::NotYetLowered { needs: Capability::Calls })
//! ```
//!
//! That refusal is the boundary working. The machine knows a generic operation
//! is a call and declines to say *which* call, because which one is a fact about
//! JavaScript.
//!
//! # Why linkage here is by name, when the engine's rule is by index
//!
//! Index linkage is right for a set one side numbers and the other reads —
//! `TABLE_HASH` exists because an index skew fails quietly where a name fails
//! loudly. This is not that set. Two crates that never see each other's source
//! must agree, and the only thing that makes them agree is that each states the
//! same symbol and the same signature.
//!
//! With names, a disagreement is an unresolved symbol at link time. With
//! indices, it is a call to the wrong function with plausible arguments. The
//! rule was never "indices everywhere" — it was that a *closed set assembled in
//! one place* should not be addressed by string. This set is assembled in two
//! places by construction.
//!
//! # What is not here
//!
//! The machine's own entry points — allocation, the write barrier, the promise
//! operations, the inline-cache miss. Those are `rts_cranelift::symbols::RtEntry`
//! and the machine emits them itself. A language layer naming them would be
//! reaching past the boundary to do work the machine already does.

use rts_cranelift::ir::{FuncId, FuncRegistry, Signature};
use rts_cranelift::repr::Repr;

use crate::emit::UNPROVEN;

/// How many argument slots a compiled function has.
///
/// # Why a constant here rather than a dynamic arity
///
/// JavaScript's arity is dynamic: a call may pass fewer arguments than the
/// function declares, or more. Expressing that needs somewhere to put an
/// argument vector, and a caller-allocated one is a stack slot this compiler
/// does not emit yet. So the convention is fixed at four, missing arguments are
/// filled with `undefined` at the call site, and a call with more is refused by
/// name — refused rather than truncated, because a call whose fifth argument
/// silently vanished is a wrong program that runs.
///
/// # Why it is stated here and again in the runtime
///
/// It is a **contract between two crates that never see each other's source**,
/// exactly like the singleton numbering and the key numbering. The language
/// decides it, because which convention compiled code uses is a fact about what
/// this crate emits; the runtime restates it because it is what performs the
/// call. `rts-host-rwk` is the one crate that may name both, and it asserts they
/// agree — which is where a disagreement becomes a refusal instead of a jump
/// with a corrupt stack.
pub const ARGUMENT_SLOTS: usize = 4;

/// An operation the language performs by calling the runtime.
///
/// Membership is decided by the machine's rule, quoted rather than paraphrased
/// because the first version of this comment shortened it to "it touches the
/// heap" — and a *narrower* rule in the crate that decides membership would emit
/// an operation as instructions when it should have been a call:
///
/// > An entry point exists if and only if the operation touches the heap, the
/// > operating system, or global mutable state. Pure computation is
/// > instructions.
///
/// — [`rts_cranelift::symbols`]. Every operation below happens to touch the
/// heap; the other two clauses are what will decide the next ones.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum RuntimeOp {
    /// `a + b`.
    ///
    /// Not addition. It converts both operands to primitives and then either
    /// concatenates or adds depending on what came back, and concatenating
    /// allocates.
    Add,

    /// `a === b`.
    ///
    /// Two strings are identical when their *text* is, which reads the heap.
    /// Everything else about strict equality is a comparison, which is why this
    /// is the only one of the three equalities with an entry point so far.
    StrictEquals,

    /// `ToBoolean`.
    ///
    /// Seven falsy values, six of which a comparison settles. The seventh is the
    /// empty string, and finding out whether a string is empty reads its length
    /// from the heap.
    ///
    /// This is what a condition needs, and it is why control flow cannot be
    /// emitted before calls: the machine's `branch` requires a proven `Bool`,
    /// and the only route from a tagged JavaScript value to one runs through
    /// here.
    ToBoolean,

    /// `String(n)`.
    ///
    /// The result is allocated.
    NumberToString,

    /// `a - b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Subtract,

    /// `a * b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Multiply,

    /// `a / b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Divide,

    /// `a % b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Remainder,

    /// `a < b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Less,

    /// `a <= b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    LessEqual,

    /// `a > b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    Greater,

    /// `a >= b`.
    ///
    /// An entry point because `ToNumber` of a string reads the heap. The
    /// operation once both operands are numbers is one instruction, and
    /// proving that is what a type pass buys.
    GreaterEqual,

    /// `{}` — a new object.
    ///
    /// An entry point without argument: making an object allocates, reading a
    /// property walks the heap, and writing one may move the object to a new
    /// layout.
    ObjectNew,

    /// `o.x` — read a property named by its key number.
    ///
    /// An entry point without argument: making an object allocates, reading a
    /// property walks the heap, and writing one may move the object to a new
    /// layout.
    GetProperty,

    /// `o.x = v` — write one, and answer the value.
    ///
    /// An entry point without argument: making an object allocates, reading a
    /// property walks the heap, and writing one may move the object to a new
    /// layout.
    SetProperty,

    /// A function, as a value: its code and the environment it closed over.
    ///
    /// An entry point because the result is allocated. The code address is not
    /// computed here — it comes from the machine's `FuncAddr`, which is a
    /// relocation the destination fills in.
    ClosureNew,

    /// A string literal, named by the number the compilation gave it.
    ///
    /// An entry point because a string is a heap value and two occurrences of
    /// `"a"` in a program are the same one — which is interning, which reads a
    /// table the runtime owns. The number is not the string: it is which
    /// literal, in the table the host seeded from what this compilation
    /// collected.
    StringConst,

    /// `typeof v`.
    ///
    /// An entry point because the answer is a string, and every one of the
    /// eight it can produce is allocated the same way any other is. It is also
    /// the only read in the language that cannot fail, which is a fact about
    /// the operand rather than about the result.
    TypeOf,

    /// `a == b`.
    ///
    /// Not `===` with conversions bolted on: it has its own table, and the
    /// part of it that reads a string.s text is what makes it an entry point.
    LooseEquals,

    /// `a ** b`.
    Exponent,

    /// `a & b`. `ToInt32` runs `ToNumber` first, which reads the heap.
    BitAnd,

    /// `a | b`.
    BitOr,

    /// `a ^ b`.
    BitXor,

    /// `~a`.
    BitNot,

    /// `a << b`.
    ShiftLeft,

    /// `a >> b`.
    ShiftRight,

    /// `a >>> b`.
    ShiftRightUnsigned,

    /// `o[e]` — read a property whose name the program computed.
    ///
    /// A separate operation from the named read rather than the same one with
    /// a different argument: the named one carries a key the COMPILER
    /// resolved, and this one carries a value that has to become a key while
    /// running, which is `ToPropertyKey` and interns text.
    GetIndexed,

    /// `o[e] = v`.
    SetIndexed,

    /// `k in o`.
    HasProperty,

    /// `[…]` — a new array of a known length.
    ///
    /// The length is known at the literal, so the store is sized once rather
    /// than grown per element.
    ArrayNew,

    /// `delete o.x`.
    ///
    /// Removing a property rebuilds the layout without it and moves what is
    /// left, which is why it is an entry point and why it is expensive.
    DeleteProperty,

    /// `for (k in o)` — the keys, as an array of strings.
    ///
    /// An array rather than an iterator, because an iterator is a call and an
    /// entry point cannot make one. With an array the loop is ordinary
    /// indexed machinery that already exists.
    OwnKeys,

    /// `new f(…)`.
    ///
    /// Not a call with a flag: it makes an object whose prototype is the
    /// callee's `prototype`, runs the callee with that object as `this`, and
    /// answers the object unless the callee returned one of its own.
    Construct,

    /// `v instanceof f`.
    InstanceOf,

    /// Calling a value, with a receiver and the arguments.
    ///
    /// # Why calling is a runtime operation and not `call_indirect`
    ///
    /// The machine has an indirect call and it takes a callee *proven to be
    /// code*. A JavaScript callee is a value, and finding out whether it is
    /// code reads the heap — so the narrowing is a heap operation, which is the
    /// membership rule unmodified.
    ///
    /// The sharper reason is what happens when it is not. `1()` throws a
    /// `TypeError`, and throwing needs the machine's protected regions, which
    /// nothing emits yet. Compiled code therefore has no way to fail here,
    /// while the runtime does — and the alternative, jumping through whatever
    /// the value spelled, is not a slower wrong answer but an arbitrary
    /// address.
    ///
    /// The arity is fixed at four arguments, padded with `undefined`. See
    /// `emit/call.rs` for why, and for what changes when a stack slot exists to
    /// put a real argument vector in.
    Call,

    /// `/pattern/flags` — a new regular expression.
    ///
    /// # Why the text and not a number naming a compiled pattern
    ///
    /// A table of patterns compiled at build time would serve the literal and
    /// have nothing to say about `new RegExp(s)`, where the pattern is a value.
    /// Handing over two strings is one path both spellings reach, which is rule
    /// 3: `/a/` and `new RegExp("a")` are the same operation and get one
    /// definition.
    ///
    /// # Why it is a call at all, when the pattern is known while compiling
    ///
    /// Because the *object* is not. `/a/g` evaluated twice is two objects with
    /// their own `lastIndex`, so a literal hoisted to a constant would make two
    /// passes of a loop share where the next search starts.
    RegexNew,

    /// A name the runtime provides, by the key number the compiler resolved.
    ///
    /// Not the global object: nothing here creates a property, and an unbound
    /// name is still refused. It is how a name whose value must be *allocated*
    /// — `RegExp` is an object with a `prototype`, where `NaN` is a constant —
    /// reaches the one the runtime made.
    GlobalGet,

    /// What an object inherits from.
    ///
    /// `new` links a prototype without this: it reads `F.prototype` and the
    /// runtime does the linking, both inside one operation. A class is where
    /// the two come apart — `class B extends A {}` links at DEFINITION time
    /// from a value the program computed, with no construction near it.
    GetPrototype,

    /// Links an object to what it inherits from.
    SetPrototype,

    /// Writes a global, creating it.
    ///
    /// The whole of sloppy mode.s global creation: an assignment to a name
    /// nothing declared puts a property on the global object. Strict mode
    /// throws instead, which is a `ReferenceError` this engine cannot raise.
    GlobalSet,

    /// `get x() { … }` — the getter half of an accessor.
    ///
    /// Not a property write. An accessor is a pair of functions the read has to
    /// CALL, and recording one as an ordinary property would have the cache
    /// return the function instead of its result.
    DefineGetter,

    /// `set x(v) { … }` — the setter half.
    DefineSetter,

    /// `super(…)` — the parent constructor, producing the object.
    ///
    /// Not a call with a receiver. Only the base of a chain knows what kind of
    /// object to allocate, so a derived constructor has no `this` until this
    /// answers one — which is the specification.s own shape, and why reading
    /// `this` before `super()` is a ReferenceError rather than `undefined`.
    SuperConstruct,

    /// Records that a constructor must ask its parent for the object.
    ///
    /// Whether a class has an `extends` is syntax, which this crate knows and
    /// the runtime cannot see: a derived constructor and a plain function are
    /// the same kind of cell.
    MarkDerived,

    /// Calling with more arguments than the convention carries.
    ///
    /// The arguments go in an ordinary array and the runtime holds it for the
    /// activation. Not a stack slot the compiler emits — that is the machine
    /// capability this waits for — and not this crate deciding where they live
    /// either: it says "call with these" and never learns where they went.
    CallWithArgs,

    /// `...rest` — the arguments past the declared ones, as an array.
    ///
    /// Takes the four slots as well as the count, because most calls allocate
    /// no vector and a rest parameter over four or fewer arguments has to work
    /// anyway.
    RestArguments,

    /// `new f(…)` with more arguments than the convention carries.
    ///
    /// Its own operation rather than the call one with a flag, for the reason
    /// `Construct` is: it makes the receiver rather than taking one.
    ConstructWithArgs,
}

impl RuntimeOp {
    /// Every operation, in declaration order.
    pub const ALL: &'static [RuntimeOp] = &[
        RuntimeOp::Add,
        RuntimeOp::StrictEquals,
        RuntimeOp::ToBoolean,
        RuntimeOp::NumberToString,
        RuntimeOp::Subtract,
        RuntimeOp::Multiply,
        RuntimeOp::Divide,
        RuntimeOp::Remainder,
        RuntimeOp::Less,
        RuntimeOp::LessEqual,
        RuntimeOp::Greater,
        RuntimeOp::GreaterEqual,
        RuntimeOp::ObjectNew,
        RuntimeOp::GetProperty,
        RuntimeOp::SetProperty,
        RuntimeOp::ClosureNew,
        RuntimeOp::StringConst,
        RuntimeOp::TypeOf,
        RuntimeOp::LooseEquals,
        RuntimeOp::Exponent,
        RuntimeOp::BitAnd,
        RuntimeOp::BitOr,
        RuntimeOp::BitXor,
        RuntimeOp::BitNot,
        RuntimeOp::ShiftLeft,
        RuntimeOp::ShiftRight,
        RuntimeOp::ShiftRightUnsigned,
        RuntimeOp::GetIndexed,
        RuntimeOp::SetIndexed,
        RuntimeOp::HasProperty,
        RuntimeOp::ArrayNew,
        RuntimeOp::DeleteProperty,
        RuntimeOp::OwnKeys,
        RuntimeOp::Construct,
        RuntimeOp::InstanceOf,
        RuntimeOp::Call,
        RuntimeOp::RegexNew,
        RuntimeOp::GlobalGet,
        RuntimeOp::GetPrototype,
        RuntimeOp::SetPrototype,
        RuntimeOp::GlobalSet,
        RuntimeOp::DefineGetter,
        RuntimeOp::DefineSetter,
        RuntimeOp::SuperConstruct,
        RuntimeOp::MarkDerived,
        RuntimeOp::CallWithArgs,
        RuntimeOp::RestArguments,
        RuntimeOp::ConstructWithArgs,
    ];

    /// The linker name the runtime must define.
    ///
    /// The contract between this crate and whichever runtime is linked. It is
    /// not decorated with a scope prefix: the old engine's `__rtsm_`/`__rtsn_`
    /// convention exists to organise a table of thousands, and a set a reader
    /// sees in one screen is not the failure mode that motivated it.
    pub fn symbol(self) -> &'static str {
        match self {
            RuntimeOp::Add => "__rts_add",
            RuntimeOp::StrictEquals => "__rts_strict_equals",
            RuntimeOp::ToBoolean => "__rts_to_boolean",
            RuntimeOp::NumberToString => "__rts_number_to_string",
            RuntimeOp::Subtract => "__rts_subtract",
            RuntimeOp::Multiply => "__rts_multiply",
            RuntimeOp::Divide => "__rts_divide",
            RuntimeOp::Remainder => "__rts_remainder",
            RuntimeOp::Less => "__rts_less",
            RuntimeOp::LessEqual => "__rts_less_equal",
            RuntimeOp::Greater => "__rts_greater",
            RuntimeOp::GreaterEqual => "__rts_greater_equal",
            RuntimeOp::ObjectNew => "__rts_object_new",
            RuntimeOp::GetProperty => "__rts_get_property",
            RuntimeOp::SetProperty => "__rts_set_property",
            RuntimeOp::ClosureNew => "__rts_closure_new",
            RuntimeOp::StringConst => "__rts_string_const",
            RuntimeOp::TypeOf => "__rts_type_of",
            RuntimeOp::LooseEquals => "__rts_loose_equals",
            RuntimeOp::Exponent => "__rts_exponent",
            RuntimeOp::BitAnd => "__rts_bit_and",
            RuntimeOp::BitOr => "__rts_bit_or",
            RuntimeOp::BitXor => "__rts_bit_xor",
            RuntimeOp::BitNot => "__rts_bit_not",
            RuntimeOp::ShiftLeft => "__rts_shift_left",
            RuntimeOp::ShiftRight => "__rts_shift_right",
            RuntimeOp::ShiftRightUnsigned => "__rts_shift_right_unsigned",
            RuntimeOp::GetIndexed => "__rts_get_indexed",
            RuntimeOp::SetIndexed => "__rts_set_indexed",
            RuntimeOp::HasProperty => "__rts_has_property",
            RuntimeOp::ArrayNew => "__rts_array_new",
            RuntimeOp::DeleteProperty => "__rts_delete_property",
            RuntimeOp::OwnKeys => "__rts_own_keys",
            RuntimeOp::Construct => "__rts_construct",
            RuntimeOp::InstanceOf => "__rts_instance_of",
            RuntimeOp::Call => "__rts_call",
            RuntimeOp::RegexNew => "__rts_regex_new",
            RuntimeOp::GlobalGet => "__rts_global_get",
            RuntimeOp::GetPrototype => "__rts_get_prototype",
            RuntimeOp::SetPrototype => "__rts_set_prototype",
            RuntimeOp::GlobalSet => "__rts_global_set",
            RuntimeOp::DefineGetter => "__rts_define_getter",
            RuntimeOp::DefineSetter => "__rts_define_setter",
            RuntimeOp::SuperConstruct => "__rts_super_construct",
            RuntimeOp::MarkDerived => "__rts_mark_derived",
            RuntimeOp::CallWithArgs => "__rts_call_with_args",
            RuntimeOp::RestArguments => "__rts_rest_arguments",
            RuntimeOp::ConstructWithArgs => "__rts_construct_with_args",
        }
    }

    /// What it takes and what it gives back.
    ///
    /// Written in the machine's representations rather than in JavaScript's
    /// types, because that is what a call site has to agree about. `UNPROVEN` in
    /// a parameter means "a JavaScript value, nothing established"; `Repr::Bool`
    /// in a return means the runtime has established one, which is exactly what
    /// makes `to_boolean` useful to a branch.
    pub fn signature(self) -> Signature {
        let (params, returns) = match self {
            RuntimeOp::Add => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::StrictEquals => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::ToBoolean => (vec![UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::NumberToString => (vec![Repr::F64], vec![UNPROVEN]),
            RuntimeOp::Subtract => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Multiply => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Divide => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Remainder => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::Less => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::LessEqual => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::Greater => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::GreaterEqual => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::ObjectNew => (vec![], vec![UNPROVEN]),
            RuntimeOp::GetProperty => (vec![UNPROVEN, Repr::I64], vec![UNPROVEN]),
            RuntimeOp::SetProperty => (vec![UNPROVEN, Repr::I64, UNPROVEN], vec![UNPROVEN]),
            // The code address is `I64` and not a value: it is a machine
            // address, nothing collects it, and widening it would hand the
            // collector a pointer into the text segment to trace.
            RuntimeOp::ClosureNew => (vec![Repr::I64, UNPROVEN], vec![UNPROVEN]),
            // Which literal, not the text: an index the compilation minted.
            RuntimeOp::StringConst => (vec![Repr::I64], vec![UNPROVEN]),
            RuntimeOp::TypeOf => (vec![UNPROVEN], vec![UNPROVEN]),
            // A proven boolean, like the other equalities: the runtime
            // establishes it, which is what lets a branch consume one.
            RuntimeOp::LooseEquals => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::Exponent
            | RuntimeOp::BitAnd
            | RuntimeOp::BitOr
            | RuntimeOp::BitXor
            | RuntimeOp::ShiftLeft
            | RuntimeOp::ShiftRight
            | RuntimeOp::ShiftRightUnsigned => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::BitNot => (vec![UNPROVEN], vec![UNPROVEN]),
            // The key is a VALUE, where the named read takes the number the
            // compiler resolved. That is the whole difference between them.
            RuntimeOp::GetIndexed => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::SetIndexed => (vec![UNPROVEN, UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            RuntimeOp::HasProperty => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            // A count the compiler knows, not a value: an array literal's
            // length is how many elements were written.
            RuntimeOp::ArrayNew => (vec![Repr::I64], vec![UNPROVEN]),
            RuntimeOp::DeleteProperty => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            RuntimeOp::OwnKeys => (vec![UNPROVEN], vec![UNPROVEN]),
            // The callee and the arguments — no receiver, because `new` makes
            // the one the callee gets.
            RuntimeOp::Construct => (vec![UNPROVEN; 1 + ARGUMENT_SLOTS], vec![UNPROVEN]),
            RuntimeOp::InstanceOf => (vec![UNPROVEN, UNPROVEN], vec![Repr::Bool]),
            // Callee, receiver, then one slot per argument. Every one a value,
            // because a caller cannot know what it is handing over.
            RuntimeOp::Call => (vec![UNPROVEN; 2 + ARGUMENT_SLOTS], vec![UNPROVEN]),
            // The pattern and the flags, as strings — which is what makes this
            // one operation serve both the literal and the constructor.
            RuntimeOp::RegexNew => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            // A key the compiler resolved, like the named property read — and
            // for the same reason: both sides hold the same number, so no text
            // has to cross.
            RuntimeOp::GlobalGet => (vec![Repr::I64], vec![UNPROVEN]),
            RuntimeOp::GetPrototype => (vec![UNPROVEN], vec![UNPROVEN]),
            // Answers the object, so a lowering can chain — which a class
            // definition does three times in a row.
            RuntimeOp::SetPrototype => (vec![UNPROVEN, UNPROVEN], vec![UNPROVEN]),
            // The key the compiler resolved, and the value. Answers the value,
            // because an assignment is an expression.
            RuntimeOp::GlobalSet => (vec![Repr::I64, UNPROVEN], vec![UNPROVEN]),
            // The object, the key the compiler resolved, and the function.
            // Answers the object, so a lowering can define several in a row.
            RuntimeOp::SuperConstruct => (vec![UNPROVEN; 1 + ARGUMENT_SLOTS], vec![UNPROVEN]),
            RuntimeOp::MarkDerived => (vec![UNPROVEN], vec![UNPROVEN]),
            // The callee, the receiver, and the arguments as one array.
            RuntimeOp::CallWithArgs => (vec![UNPROVEN; 3], vec![UNPROVEN]),
            // The callee and the arguments — no receiver, because `new` makes
            // the one the callee gets.
            RuntimeOp::ConstructWithArgs => (vec![UNPROVEN; 2], vec![UNPROVEN]),
            // How many are declared, then the four the convention carried.
            RuntimeOp::RestArguments => {
                (vec![Repr::I64, UNPROVEN, UNPROVEN, UNPROVEN, UNPROVEN], vec![UNPROVEN])
            }
            RuntimeOp::DefineGetter | RuntimeOp::DefineSetter => {
                (vec![UNPROVEN, Repr::I64, UNPROVEN], vec![UNPROVEN])
            }
        };
        Signature {
            params,
            returns,
            ..Signature::default()
        }
    }
}

/// The runtime operations one compilation has declared, declared at most once
/// each.
///
/// A `FuncId` is an index into a registry and carries no name, so the same
/// operation asked for twice must yield the same id or the two call sites are
/// calling two different functions as far as the machine is concerned.
///
/// A fixed-size array rather than a map: the set is closed and small enough that
/// hashing a key would cost more than the lookup it replaces, and an array
/// cannot acquire an entry that `ALL` does not list.
pub struct RuntimeCalls {
    declared: [Option<FuncId>; RuntimeOp::ALL.len()],
}

impl Default for RuntimeCalls {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeCalls {
    /// Nothing declared yet.
    pub fn new() -> Self {
        RuntimeCalls {
            declared: [None; RuntimeOp::ALL.len()],
        }
    }

    /// The id for an operation, declaring it the first time it is asked for.
    ///
    /// Lazy rather than declaring all of them up front, so a compilation that
    /// never concatenates does not carry a relocation to the string path. What
    /// a program does not do should not appear in what it links.
    pub fn declare(&mut self, funcs: &mut FuncRegistry, op: RuntimeOp) -> FuncId {
        let slot = op as usize;
        match self.declared[slot] {
            Some(id) => id,
            None => {
                let sig = funcs.declare_signature(op.signature());
                let id = funcs.declare_function(sig);
                self.declared[slot] = Some(id);
                id
            }
        }
    }

    /// Which operations were declared, in declaration order.
    ///
    /// What a linker has to resolve for this compilation. The consumer is
    /// whatever binds a `FuncId` to a symbol — today a test, and `rts-host`
    /// when it exists.
    pub fn declared(&self) -> impl Iterator<Item = (RuntimeOp, FuncId)> + '_ {
        RuntimeOp::ALL
            .iter()
            .enumerate()
            .filter_map(|(slot, op)| self.declared[slot].map(|id| (*op, id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_twice_yields_one_function_not_two() {
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        let first = calls.declare(&mut funcs, RuntimeOp::Add);
        let second = calls.declare(&mut funcs, RuntimeOp::Add);
        assert_eq!(
            first, second,
            "a FuncId carries no name, so two ids for one operation are two \
             different functions as far as the machine can tell"
        );
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn nothing_is_declared_until_something_asks() {
        let mut funcs = FuncRegistry::new();
        let mut calls = RuntimeCalls::new();
        calls.declare(&mut funcs, RuntimeOp::Add);
        // A program that never concatenates must not carry a relocation to the
        // string path, and a compilation that declared everything up front
        // would link every operation into every program.
        assert_eq!(calls.declared().count(), 1);
        assert_eq!(funcs.len(), 1);
    }

    #[test]
    fn the_slot_is_the_discriminant_and_the_list_agrees_with_it() {
        // `declare` indexes by `op as usize`, so a variant added to the enum
        // without being added to ALL — or added to ALL out of order — would
        // write one operation's id into another's slot. Silent, and this is the
        // check that makes it loud.
        for (position, op) in RuntimeOp::ALL.iter().enumerate() {
            assert_eq!(*op as usize, position, "{op:?} is out of place in ALL");
        }
    }

    #[test]
    fn to_boolean_returns_a_proven_boolean_because_a_branch_needs_one() {
        // The reason control flow could not be emitted before calls, stated as
        // a test rather than only as prose: the machine's `branch` refuses
        // anything but `Repr::Bool`, and this is the only route to one from a
        // tagged JavaScript value.
        assert_eq!(RuntimeOp::ToBoolean.signature().returns, vec![Repr::Bool]);
        assert_eq!(RuntimeOp::ToBoolean.signature().params, vec![UNPROVEN]);
    }

    #[test]
    fn every_symbol_is_distinct() {
        let mut seen: Vec<&str> = RuntimeOp::ALL.iter().map(|op| op.symbol()).collect();
        seen.sort_unstable();
        let count = seen.len();
        seen.dedup();
        assert_eq!(
            seen.len(),
            count,
            "two operations claiming one symbol link to one function"
        );
    }
}
