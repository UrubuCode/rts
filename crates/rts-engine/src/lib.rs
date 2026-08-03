//! `rts-engine` — o núcleo cru do RTS.
//!
//! Este crate é o **motor**: um *registry* programático + um *builder* que o
//! codegen consome para resolver `module.member(args)`, `Class.method(...)` e
//! globais bare, **sem nada hardcoded lá dentro**. Toda a superfície da
//! linguagem é registrada aqui — não há tabela `const` escrita à mão nem
//! `proc-macro` derivando externs.
//!
//! ## Por que substitui o `rts-macro`
//!
//! O modelo antigo (arrays `SPECS`/`GLOBAL_CLASS_SPECS` estáticos + a lista
//! `add_fn!` do JIT, todos deletados no cutover) tinha **três fontes de verdade
//! à mão** se desincronizando
//! (ver `docs/engine/architecture.md` §4). O builder unifica isso: cada item é registrado
//! **uma vez**, carregando o **ponteiro de função nativo** (que dobra como
//! símbolo do JIT) + a assinatura ABI. Builtins e módulos externos (`.dll`/
//! `.so`, ver `docs/engine/architecture.md` §10) usam o **mesmo** caminho de registro.
//!
//! Trade honesto: perde-se a auto-derivação da macro (extern + metadata de uma
//! assinatura); ganha-se um caminho uniforme + a morte dos arrays-à-mão. O
//! helper [`sig!`] devolve a ergonomia da assinatura sem proc-macro.
//!
//! ## Camadas (futuro)
//!
//! ```text
//!   rts-engine   ── registry + builder + (futuro) gc + globals   [núcleo cru]
//!      ▲  ▲  ▲
//!      │  │  └── rts-browser  (globais + módulos puros de navegador)
//!      │  └───── rts-node     (compat node:)
//!      └──────── rts-std      (io/fs/math/... = as namespaces de hoje)
//! ```
//! Cada camada recebe um [`Engine`] e chama `engine.module(...)` /
//! `engine.class(...)` / `engine.global(...)` para registrar a sua superfície.
//! O codegen lê o [`Registry`] resultante; ele não sabe (nem precisa) de qual
//! camada veio cada módulo.
//!
//! ## Estado
//!
//! Bootstrap: o registry + o builder + a resolução chaveada-por-aridade. A
//! migração de GC, globals, e das namespaces (`rts-std`) para cima deste núcleo
//! é o roadmap de `docs/engine/architecture.md` (F3/F4 + a virada de autoria).

// `#[rtse::*]` emits `::rts_engine::*` paths, so this crate must be able to name
// ITSELF by that path — the same reason `rts-runtime` does it for `adapters/`.
extern crate self as rts_engine;

pub mod abi;
pub mod loop_sources;
pub mod runtime_ci;
pub mod watch_queue;

mod builder;
mod member;
mod registry;
mod sig;

// HOW IT WORKS INSIDE — re-exported verbatim from `rts-natives`, which now owns
// the heap/HandleTable/shapes/NaN-box, the GC and the number formatter
// (RTS_ORGANIZATION.md N1/N2). The engine MANAGES dispatch; it does not own the
// value representation. Re-exporting rather than relocating the call sites is
// deliberate: `rts_engine::heap::…` is what ~15 crates and every `#[rtse::*]`
// expansion above already write, and a pure move must be invisible to them.
pub use rts_natives::{collector, externs, heap, numfmt};

pub use builder::{
    ClassBuilder, Engine, GlobalBuilder, GlobalRegistryItem, GlobalScope, ModuleBuilder,
    ModuleScope, RegistryItem,
};
pub use rts_natives::{GcPayload, Traceable};
pub use member::{FnPtr, Member, VarKind, NativeEmit};
pub use registry::{Class, Module, Registry};
pub use sig::Sig;

/// Drive a `Future` to completion synchronously. The `#[rtse::asynch]` bridge
/// calls this to run a class's real Rust `async fn` body to a value (interim
/// engine async is synchronous), which is then wrapped in a settled JS Promise.
/// Pure pollster executor — no tokio, no C, wasm-safe.
pub fn block_on<F: core::future::Future>(fut: F) -> F::Output {
    pollster::block_on(fut)
}

// Vocabulário ABI estável (antes o crate `rts-abi`, agora dobrado em `abi`).
// Re-exportado na raiz para os crates de camada e o codegen.
pub use abi::{
    AbiType, DefaultArg, JsErrorKind, MemberFlags, MemberKind,
    NamespaceMember, NamespaceSpec, concat_members,
};
