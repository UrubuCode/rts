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
//!  - Dotted class names (`Intl.NumberFormat`) sanitize `.`→`_` in symbols.
//!  - Symbol member/field names are VERBATIM (case-preserved), not uppercased, so
//!    `fn Foo` vs `fn foo` stay distinct symbols. Consumers link by the exact name.
//!  - `-> Option<Self>` (fallible ctor/factory → JS null on `None`) and
//!    `-> Option<String>` (nullable string → JS null, ts `string | null`).
//! Open gaps: `#[rtse::symbol]` (well-known), constants, `global(descriptor)`, a
//! per-param custom omitted-default sentinel (Date's `i64::MIN` "keep current").
//!
//! # Module map
//!
//! `rustc` requires every `#[proc_macro_attribute]` fn to be a top-level item of
//! the crate (a `pub use` re-export of one declared in a submodule does not
//! satisfy that), so this file stays the set of thin entry points — arg parsing
//! and expansion are delegated to submodules that hold no proc-macro items of
//! their own:
//!  - `class` — `#[rtse::class]` expansion (impl form + struct form).
//!  - `abi` — `#[rtse::abi]` expansion (extern-C symbol + `SymbolDesc` const).
//!  - `types` — Rust-type → `AbiType`/ts-type mapping shared by both.
//!  - `naming` — symbol/const name derivation (`to_camel`, `abi_const_name`).
//! The inert marker attributes (`method`, `ctor`, …) are trivial passthroughs and
//! stay here rather than in their own module for the same top-level-item reason.

use proc_macro::TokenStream;

mod abi;
mod class;
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
/// with the ABI shape derived from the Rust signature. See `abi::expand`'s doc
/// for the full rationale.
#[proc_macro_attribute]
pub fn abi(a: TokenStream, item: TokenStream) -> TokenStream {
    abi::expand(a, item)
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
#[proc_macro_attribute]
pub fn symbol(_a: TokenStream, item: TokenStream) -> TokenStream {
    item
}
