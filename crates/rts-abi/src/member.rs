//! Namespace member metadata.
//!
//! Each `NamespaceMember` is consumed by codegen to emit direct
//! `call <symbol>` instructions and by the TypeScript declaration generator
//! to produce `rts.d.ts`. The layout intentionally mirrors what both
//! consumers need so no additional lookup structure is required.

use super::types::AbiType;

/// One exported entry inside a namespace.
///
/// Lifetime is `'static`: all entries live in `const` tables compiled into
/// the binary. No heap allocation occurs for metadata.
#[derive(Debug, Clone, Copy)]
pub struct NamespaceMember {
    /// Public name as written in TypeScript, e.g. `"print"`.
    pub name: &'static str,

    /// Whether the member is a callable function or a constant value.
    pub kind: MemberKind,

    /// Canonical C symbol. Must satisfy `abi::symbols::validate_symbol`.
    pub symbol: &'static str,

    /// Argument types, in order, as they appear in the function signature.
    /// `StrPtr` contributes two slots — codegen expands them automatically.
    pub args: &'static [AbiType],

    /// Return type. `AbiType::Void` indicates no return value.
    pub returns: AbiType,

    /// Doc comment used by `rts.d.ts` generation.
    pub doc: &'static str,

    /// TypeScript signature text, e.g. `"print(msg: string): void"`.
    pub ts_signature: &'static str,


    /// True if this member is pure: no I/O, no shared mutable state, no
    /// non-determinism. Pure members are eligible for automatic parallelisation
    /// (e.g. `for...of` body analysis in the purity pass). Conservative:
    /// false-negative is safe (falls back to sequential path).
    pub pure: bool,

    /// Alternative JS-visible names that resolve to this same member, e.g.
    /// `toLocaleLowerCase` aliasing `toLowerCase`, or the snake_case ergonomic
    /// aliases (`starts_with`, `trim_start`). Empty for most members. Lets the
    /// engine resolver treat aliases as data instead of `|`-joined match arms.
    pub aliases: &'static [&'static str],

    /// True when the last logical parameter is variadic (JS rest `...args`).
    /// The generic call emitter cannot marshal these 1:1; codegen routes
    /// variadic members to a residual handler (per-element loop). Default false.
    pub variadic: bool,

    /// Per-argument default policy, parallel to `args` (same length, or empty
    /// meaning "all required"). `DefaultArg::Required` = caller must supply it;
    /// an optional variant carries the value the emitter injects when the caller
    /// omits the argument (e.g. `String.prototype.slice` end → an
    /// end-of-string sentinel). Enables arity-overload resolution that tolerates
    /// an optional tail. Empty for members with no optional params.
    pub default_args: &'static [DefaultArg],

    /// Modifier bitset (readonly / static-field / mutable). `MemberFlags::NONE`
    /// for the overwhelming majority. Codegen reads this to enforce write
    /// rejection on `READONLY` and to pick the atomic-backed getter/setter path
    /// on `MUTABLE`. Visibility (`#private`/protected) is intentionally NOT a
    /// flag here — a builtin member that user TS must not reach is simply not
    /// declared as a public member (see `docs/specs/rts-engine-dispatch.md` §9.5).
    pub flags: MemberFlags,
}

/// Member modifier bitset. Plain `u8` newtype so it is const-constructible and
/// `Copy`, keeping `NamespaceMember` allocation-free per the `'static const`
/// table invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberFlags(u8);

impl MemberFlags {
    /// No modifiers — the default for ordinary members.
    pub const NONE: Self = Self(0);
    /// Assignment to this member is a hard codegen error. Stamped by the macro
    /// on every getter that has no paired setter, and on `#[rts_var(const)]`.
    pub const READONLY: Self = Self(1 << 0);
    /// A mutable *static field* (vs a read-once `Constant`).
    pub const STATIC: Self = Self(1 << 1);
    /// Backing storage is a process-global atomic variable (`#[rts_var]`
    /// `let`/`var`); read/write lower to the atomic getter/setter externs.
    pub const MUTABLE: Self = Self(1 << 2);
    /// O segundo arg (após o `fn_ptr`) passa os BITS do `f64` (`bitcast i64`),
    /// não a conversão numérica — trampolins que recebem `arg: number` via param
    /// f64 (`thread.spawn*`). Substitui o match por símbolo em `ns_call.rs` (Q2).
    pub const RAW_BITS_ARG: Self = Self(1 << 3);
    /// O retorno `I64` pode ser handle (string/obj) em vez de inteiro — marca
    /// como ambíguo p/ template/console usarem `TPL_COERCE_AUTO`
    /// (`parallel.find/reduce*`, `promise.wait`, `JSON.parse*`). (Q2)
    pub const AMBIGUOUS_RET: Self = Self(1 << 4);
    /// Membro `void` que em JS retorna `undefined` — emite sentinela
    /// `i64::MIN + 2` (ambígua) em vez de `0` (`parallel.for_each`). (Q2)
    pub const UNDEF_RET: Self = Self(1 << 5);
    /// Member is registered but NOT sound to lower (timezone-divergent /
    /// mutating / non-deterministic) — codegen must BAIL rather than emit a
    /// wrong-but-close value. The honesty floor as DATA on the spec, so the
    /// engine no longer hardcodes a per-class divergent-method predicate. Today
    /// stamped on `Date`'s `toString`/`toLocale*`/`toDateString`/`toUTCString`/
    /// `toTimeString` formatters and every `setX` mutator.
    pub const UNSOUND: Self = Self(1 << 6);
    /// Member can THROW: the impl may set the pending-error slot
    /// (`__RTS_FN_RT_ERROR_SET`) — codegen emits the post-call error check
    /// after the call so a `try/catch` routes the unwind (e.g. a
    /// `{fatal: true}` TextDecoder rejecting malformed input with TypeError).
    pub const THROWS: Self = Self(1 << 7);

    /// Union of two flag sets (const, for building composite literals).
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True when every bit in `f` is set in `self`.
    pub const fn contains(self, f: Self) -> bool {
        self.0 & f.0 == f.0
    }
}

/// Default policy for one argument of a [`NamespaceMember`], used by the engine
/// resolver/emitter to accept calls that omit trailing optional arguments and
/// to synthesise the value the runtime fn expects in their place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultArg {
    /// The caller must supply this argument; omitting it is an arity error.
    Required,
    /// Optional integer/handle/bool argument; `value` is injected when omitted.
    Int(i64),
    /// Optional float argument; `value` is injected when omitted.
    Float(f64),
    /// Omitted arg defaults to JS `undefined`: codegen injects the `undefined`
    /// sentinel word and the runtime extern interprets the resulting NaN/sentinel
    /// as ITS OWN default — e.g. Date's calendar-field constructor reads a NaN
    /// component as day=1/rest=0. Lets a spec express "pad the tail with
    /// `undefined`" as DATA instead of a hardcoded codegen padding.
    Undefined,
}

/// Whether a member is a function, constant, constructor, or instance method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    /// Callable extern "C" function in a namespace (`rts:string`, etc.).
    Function,
    /// Constant value resolved once at program startup. `args` must be empty
    /// and `returns` holds the value type.
    Constant,
    /// Class constructor — emitted for `new ClassName(args)`.
    ///
    /// `args` describes the constructor parameters; `returns` is always
    /// `AbiType::Handle` (the newly allocated instance handle).
    /// Multiple `Constructor` members on the same registered `Class` are
    /// distinguished by `args.len()` (overload by arity).
    Constructor,
    /// Instance method — first ABI argument is always the `Handle` of the
    /// receiver (implicit `this`). Codegen inserts it automatically; the TS
    /// signature lists only the explicit params.
    InstanceMethod,
    /// Static method on a class — no receiver. `ClassName.method(args)`.
    /// Distinct from `Function` to separate namespace fns from class statics.
    StaticMethod,
    /// Read-only property access on an instance — `instance.prop`.
    /// ABI: `fn(handle: u64) -> T`. No parens in TS.
    InstanceGetter,
    /// Writable property on an instance — `instance.prop = v`. ABI:
    /// `fn(handle: u64, value: T)`. Pairs with an `InstanceGetter` of the same
    /// `name`. A getter with no matching `InstanceSetter` is read-only.
    InstanceSetter,
    /// Read of a mutable module-level variable — `ns.v`. ABI: `fn() -> T`.
    /// Lowered exactly like `Constant` (a zero-arg getter call), but its value
    /// is not constant. Pairs with a `VarSetter` of the same `name` unless the
    /// variable was declared `const` (then it carries `MemberFlags::READONLY`).
    VarGetter,
    /// Write of a mutable module-level variable — `ns.v = e`. ABI:
    /// `fn(value: T)`. Absent for `const` variables, making the write a hard
    /// codegen error.
    VarSetter,
}

/// Concatenates two member slices into one fixed-size array at const time.
///
/// Used by namespaces whose members are declared across multiple `#[rts_namespace(ns, part)]`
/// impl blocks (e.g. `collections` splits `map`/`vec` into separate files). `N`
/// must equal `a.len() + b.len()`; a mismatch is a const-eval out-of-bounds error
/// at compile time. Both slices must be non-empty (the array seed is `a[0]`).
pub const fn concat_members<const N: usize>(
    a: &[NamespaceMember],
    b: &[NamespaceMember],
) -> [NamespaceMember; N] {
    let mut out = [a[0]; N];
    let mut i = 0;
    while i < a.len() {
        out[i] = a[i];
        i += 1;
    }
    let mut j = 0;
    while j < b.len() {
        out[a.len() + j] = b[j];
        j += 1;
    }
    out
}

/// A registered namespace exposed through the new ABI.
#[derive(Debug, Clone, Copy)]
pub struct NamespaceSpec {
    /// Public name, e.g. `"io"`, `"fs"`, `"net"`.
    pub name: &'static str,
    /// Summary shown in `rts apis` and in generated declarations.
    pub doc: &'static str,
    /// Members of this namespace. Order is stable and significant for
    /// reproducible codegen.
    pub members: &'static [NamespaceMember],
}
