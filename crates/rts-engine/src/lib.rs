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
//! O modelo antigo (`rts-macro` + arrays `SPECS`/`GLOBAL_CLASS_SPECS` + a lista
//! `add_fn!` do JIT) tinha **três fontes de verdade à mão** se desincronizando
//! (ver `docs/specs/rts-engine-dispatch.md` §4). O builder unifica isso: cada item é registrado
//! **uma vez**, carregando o **ponteiro de função nativo** (que dobra como
//! símbolo do JIT) + a assinatura ABI. Builtins e módulos externos (`.dll`/
//! `.so`, ver `docs/specs/rts-engine-dispatch.md` §10) usam o **mesmo** caminho de registro.
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
//! é o roadmap de `docs/specs/rts-engine-dispatch.md` (F3/F4 + a virada de autoria).

pub mod abi;
pub mod collector;
pub mod heap;
pub mod numfmt;
pub mod runtime_ci;

mod builder;
mod member;
mod registry;
mod sig;

pub use builder::{ClassBuilder, Engine, GlobalBuilder, ModuleBuilder};
pub use collector::{GcPayload, Traceable};
pub use member::{FnPtr, Member, VarKind};
pub use registry::{Class, Module, Registry};
pub use sig::Sig;

// Vocabulário ABI estável (antes o crate `rts-abi`, agora dobrado em `abi`).
// Re-exportado na raiz para os crates de camada e o codegen.
pub use abi::{
    concat_members, AbiType, DefaultArg, GlobalClassSpec, Intrinsic, JsErrorKind, MemberFlags,
    MemberKind, NamespaceMember, NamespaceSpec,
};
