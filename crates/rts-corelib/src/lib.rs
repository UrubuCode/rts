//! `rts-corelib` — the neutral core: the machine primitives every front end
//! lowers to directly, below any language.
//!
//! # Why this is NOT a module
//!
//! The first cut installed an `rts:core` namespace through
//! `declare_module`/`make_namespace`. That is the *library* door — a namespace
//! object looked up by specifier at run time, which is a JavaScript construct
//! (`import … from`). The core is not a library: it is the floor of the machine,
//! reached by a DIRECT call the compiler emits, with no lookup. And rts targets
//! more than one language (`rts-mir`'s `Domain` is "implemented once per front
//! end"), so the core cannot be spelled in, or loaded by, any one of them.
//!
//! So this crate holds no `install`. It is the **catalogue** of the neutral
//! primitives — the ones a program cannot write because they *are* the floor —
//! that each front end's `Domain` maps its own names onto and lowers directly
//! (the way `Callee::Entry` already lowers to `ops.entry(...)`). A JavaScript
//! `import { … } from "rts:core"`, if offered, is compile-time sugar the JS front
//! end resolves to these primitives; Lua would bind them its own way.
//!
//! # What goes here, and what does not
//!
//! Here: the language-agnostic primitives — raw allocation, raw memory load and
//! store, slot access, the integer and bit operations, bit reinterpretation.
//! The implementations live in `rts-core`/`rts-cranelift` (availability decides
//! membership — see the `add-builtin-class` skill); this catalogue names them and
//! records the safe/`unsafe` boundary.
//!
//! Not here: anything semantic (JavaScript's `Math`, prototype chains, the module
//! table). That is a front end's, declared through its `Domain`.
//!
//! # Status
//!
//! Catalogue only. The primitives are bound to the front ends through the
//! `@intrinsic`/`@repr` surface (see `.docs-pendentes/`), which depends on the
//! int32 arithmetic path landing first. Until a primitive has a binding it is
//! listed here and wired nowhere.

/// A neutral core primitive — named here, lowered directly by each front end.
///
/// This is the catalogue, not the wiring: a variant names a primitive the core
/// guarantees on every target. The machine op or entry it lowers to lives in
/// `rts-cranelift`/`rts-core`; the mapping from a language's surface name to one
/// of these is the front end's `Domain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum CorePrimitive {
    /// Raw allocation of a cell (the bump/`rts_alloc` floor). `unsafe`: internal.
    Alloc,
    /// Read a machine word from a slot. `unsafe`: internal.
    SlotLoad,
    /// Write a machine word to a slot. `unsafe`: internal.
    SlotStore,
    /// The element count of an array-like. Safe.
    Length,
    /// 32-bit integer add with overflow side-exit. Safe.
    I32Add,
    /// 32-bit wrapping integer multiply (`Math.imul` semantics). Safe.
    ///
    /// Scaffolding: this duplicates `Math.imul`. It exists so the `rts:core`
    /// name -> primitive -> op mechanism has a first inline, safe entry; the
    /// payoff is the later (unsafe) primitives that reuse that mechanism.
    I32Mul,
    /// Leading zero bits of the 32-bit view (`Math.clz32` semantics, 0..=32). Safe.
    ///
    /// The bit-scan primitive std needs without reaching for the `Math` object.
    Clz32,
    /// Round to the nearest float32 and back to a double (`Math.fround`). Safe.
    ///
    /// The float32 primitive std needs without reaching for the `Math` object.
    Fround,
    /// Square root of a double (`Math.sqrt`). Safe.
    Sqrt,
    /// Round toward negative infinity (`Math.floor`). Safe.
    Floor,
    /// Round toward positive infinity (`Math.ceil`). Safe.
    Ceil,
    /// Round toward zero (`Math.trunc`). Safe.
    Trunc,
    /// Absolute value (`Math.abs`). Safe.
    Abs,
    /// -1, -0, +0, +1 or NaN by sign (`Math.sign`). Safe.
    Sign,
    /// The lesser of two doubles (`Math.min` semantics: NaN wins, -0 < +0). Safe.
    Min,
    /// The greater of two doubles (`Math.max` semantics: NaN wins, +0 > -0). Safe.
    Max,
    /// IEEE-754 bit pattern of a number. Safe.
    F64ToBits,
    /// A number from its IEEE-754 bit pattern. Safe.
    F64FromBits,
}

impl CorePrimitive {
    /// Whether a front end may expose this to user code, or keep it internal to
    /// the standard library. The `unsafe` primitives touch raw memory, which the
    /// conservative-scan GC cannot be handed to arbitrary programs.
    pub fn is_public(self) -> bool {
        matches!(
            self,
            Self::Length
                | Self::I32Add
                | Self::I32Mul
                | Self::Clz32
                | Self::Fround
                | Self::Sqrt
                | Self::Floor
                | Self::Ceil
                | Self::Trunc
                | Self::Abs
                | Self::Sign
                | Self::Min
                | Self::Max
                | Self::F64ToBits
                | Self::F64FromBits
        )
    }
}
