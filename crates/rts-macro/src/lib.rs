//! `#[rtse::*]` — authoring macros for the engine ABI (RTS_ENGINE_ABI_CODEGEN).
//!
//! Annotate a normal Rust struct + `impl`; `#[rtse::class(...)]` on the impl block
//! generates the extern-C ABI wrappers + Registry metadata for the JS engine,
//! WITHOUT changing the struct's normal Rust usability (like PyO3/napi-rs).
//!
//! ```ignore
//! struct Point { x: f64, y: f64 }
//! #[rtse::class("Point")]
//! impl Point {
//!     #[rtse::ctor]   fn new(x: f64, y: f64) -> Self { Point { x, y } }
//!     #[rtse::method] fn sum(&self) -> f64 { self.x + self.y }
//!     #[rtse::method] fn label(&self) -> String { format!("({},{})", self.x, self.y) }
//! }
//! ```
//!
//! Emits: the impl unchanged (methods stay normal Rust), one `extern "C"` wrapper
//! per ctor/method (marshalling handle↔struct and Rust↔ABI — a `String`/`&str`
//! return is allocated into the string pool and returned as a handle), and
//! `pub fn register(e: &mut Engine)`. The author adds `register` to `REGISTER`.
//!
//! Status — base: ctor + instance `&self`/`&mut self` methods + `#[rtse::statical]`
//! statics (`statical`, not the `static` keyword) + `#[rtse::variable]` scalar fields
//! (getter/setter) + `#[rtse::private]` + AOT force-keep (`#[used]` FnPtr array).
//! Params: `f64`/`i64`/`i32`/`bool`/`&str`. Returns: `String`/`&str`/`()`. Plus:
//!  - **F1** `Handle`/`U64` (u64 passthrough) params + returns.
//!  - **F2** overload by arity (N members same JS `name=`, distinct argc — free;
//!    the engine dispatches by argc).
//!  - **F3** `#[rtse::asynch]` on a real `async fn` — driven by `rts_engine::block_on`,
//!    return wrapped in a settled Promise (rejected on a pending error slot).
//!  - **F4** `optional=N` — last N params default `undefined` (Sig::with_defaults).
//!  - `#[rtse::getter]`/`#[rtse::setter]` — real InstanceGetter/InstanceSetter,
//!    String-capable computed properties.
//!  - `throws` — sets `MemberFlags::THROWS` (composes with readonly/optional).
//!  - **F8** `Vec<String>`/`Vec<Handle>` return → `Entry::Vec` + ts `T[]` (real array).
//!  - Instance methods CLONE the receiver out of the HandleTable before the body
//!    (drops the shard lock; write-back for `&mut self`) so a body touching a 2nd
//!    handle can't self-deadlock — the struct must be `Clone`.
//!  - Class-member symbols follow the ONE convention derived in `abi::scope`:
//!    `__rtsm_global_<class>_<member>`. The class segment is lower-cased and
//!    `_`-joined (`Intl.NumberFormat` → `intl_numberformat`); the member segment
//!    is VERBATIM (case-preserved), so `fn Foo` vs `fn foo` stay distinct. A
//!    `#[rtse::variable]` field's accessors suffix the pair: `…_<field>__get` /
//!    `…_<field>__set`.
//!  - `-> Option<Self>` (fallible ctor/factory → JS null on `None`) and
//!    `-> Option<String>` (nullable string → JS null, ts `string | null`).
//!  - `#[rtse::constant]` — a Rust `const` as a Registry `MemberKind::Constant`.
//!  - `#[rtse::symbol(...)]` — stacks on a `#[rtse::method]`/`#[rtse::getter]`/…
//!    member to key it by SYMBOL instead of a plain string name.
//!    `#[rtse::symbol("foo")]` → a REGISTRY symbol (`Symbol.for("foo")`,
//!    string-keyed); `#[rtse::symbol(Symbol::iterator)]` → a WELL-KNOWN symbol
//!    (`Symbol.iterator`, unique identity) — `Symbol` is emitted verbatim, so
//!    `rustc` checks the path resolves (see `class::kind::SymbolKey`,
//!    `rts_primitives::symbol::wellknown::Symbol`). Both forms key the member as
//!    `@@sym:<name>` / `@@<ident>` — the same `@@`-prefixed spelling the
//!    engine's computed-key desugar already uses for a `.ts` class's literal
//!    `[Symbol.iterator]()`, so a Rust member and a `.ts` member resolve through
//!    ONE protocol path, not two.
//! Open gaps: `global(descriptor)`, a per-param custom omitted-default sentinel
//! (Date's `i64::MIN` "keep current").
//!
//! # Module map
//!
//! `rustc` requires every `#[proc_macro_attribute]` fn to be a top-level item of
//! the crate (a `pub use` re-export of one declared in a submodule does not
//! satisfy that), so this file stays the set of thin entry points — arg parsing
//! and expansion are delegated to submodules that hold no proc-macro items of
//! their own:
//!  - `class` — `#[rtse::class]` expansion (impl form + struct form).
//!  - `abi` — `#[rtse::abi]` expansion: `abi::scope` parses the declared scope
//!    and DERIVES the linker symbol (`__rtsm_` module / `__rtsn_` native /
//!    `__rtsa_` ABI-contract); `abi::params` maps the Rust signature to ABI
//!    slots; `abi::expand` emits the symbol + its `SymbolDesc` const.
//!  - `constant` — `#[rtse::constant]` expansion: a Rust `const` → getter symbol
//!    + `SymbolDesc` + a `Member` the owning declaration pushes with `.member()`.
//!  - `function` — `#[rtse::function]` expansion: a FREE function (no receiver,
//!    no class) → extern symbol + `SymbolDesc` + a `Member` pushed with
//!    `.registry(...)` on the closure-scoped `ModuleScope`.
//!  - `types` — Rust-type → `AbiType`/ts-type mapping shared by both.
//!  - `naming` — symbol/const name derivation (`to_camel`, `abi_const_name`).
//! The inert marker attributes (`method`, `ctor`, …) are trivial passthroughs and
//! stay here rather than in their own module for the same top-level-item reason.

use proc_macro::TokenStream;

mod abi;
mod class;
mod constant;
mod function;
mod naming;
mod types;

/// `#[rtse::class("Name")]` — on the STRUCT (fields via `#[rtse::variable]`) OR on
/// the `impl` block (ctor/methods). See the crate doc for the full picture;
/// expansion itself lives in `class::expand`.
#[proc_macro_attribute]
pub fn class(args: TokenStream, item: TokenStream) -> TokenStream {
    class::expand(args, item)
}

/// `#[rtse::abi]` — emit a `SymbolDesc` const next to an `extern "C"` symbol,
/// with BOTH the linker name (from the declared scope: `module`/`global`/
/// `native`/`abi`) and the ABI shape derived from the Rust signature. See the
/// `abi` module doc for the naming convention and the parameter-kind table.
#[proc_macro_attribute]
pub fn abi(a: TokenStream, item: TokenStream) -> TokenStream {
    abi::expand(a, item)
}

/// `#[rtse::constant]` — expose a Rust `const` as a Registry constant, reachable
/// as `import { exemplo } from "<module>"` or as the bare global `exemplo`. Takes
/// the same scope arguments as `#[rtse::abi]`; see the `constant` module doc for
/// the emitted items, the ident→JS-name rule, and the supported types.
#[proc_macro_attribute]
pub fn constant(a: TokenStream, item: TokenStream) -> TokenStream {
    constant::expand(a, item)
}

/// `#[rtse::function]` — a FREE function member (no receiver, no class): `import
/// { readFile } from "node:fs"`. Takes the same scope arguments as
/// `#[rtse::abi]`/`#[rtse::constant]`; see the `function` module doc for the
/// emitted items and why the Registry entry is a `fn`, not a `const`.
#[proc_macro_attribute]
pub fn function(a: TokenStream, item: TokenStream) -> TokenStream {
    function::expand(a, item)
}

// Standalone marker attrs (inert — `#[rtse::class]` on the impl strips them).
#[proc_macro_attribute]
pub fn method(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn ctor(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn variable(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn private(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn statical(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn asynch(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn getter(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn setter(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
#[proc_macro_attribute]
pub fn instanceof(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
// `#[rtse::symbol(...)]` is stripped and interpreted by `class::kind::take_symbol_key`
// while `#[rtse::class]` expands the enclosing impl (same pattern as the other
// markers above) — this entry only exists so a stray/standalone use still
// parses instead of erroring "unknown attribute".
#[proc_macro_attribute]
pub fn symbol(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
