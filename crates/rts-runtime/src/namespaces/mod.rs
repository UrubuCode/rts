//! Namespace implementations exposed through the new ABI.
//!
//! Each submodule registers an `abi::SPEC` consumed by codegen via
//! [`crate::abi::SPECS`]. No legacy dispatch path remains: every callee is
//! resolved to a canonical `__RTS_*` extern "C" symbol and called directly.

pub use rts_shared::alloc;
/// `audio`/`asio_audio` migraram pro crate backend `rts-std` (Fase 1b, partição
/// de crates). Facade → `crate::namespaces::audio` (register_builtins, jit.rs)
/// segue resolvendo.
pub use rts_std::audio;
#[cfg(feature = "asio")]
pub use rts_std::asio_audio;
/// `dom` — DOM retido HEADLESS (parser HTML + árvore + query/mutação) na crate
/// separada `rts-dom`, sem UI. Resolve via Registry: `ns::dom::register`.
/// `import { parseHtml } from "rts:dom"`. O `rts-egui` consome o tipo `Dom`
/// direto (não pela ABI). Ver docs/specs/rts-dom-crate-design.md.
pub use rts_dom as dom;
/// `render` — interface de render ABSTRATA na crate `rts-render`. O DOM/layout
/// chama `render.*` (rect/text/measureText/...); o backend ativo (egui) pinta. O
/// engine nunca nomeia o backend concreto. Ver docs/specs/dom-render-input-interfaces.md.
pub use rts_render as render;
/// `egui` — GUI imediata (egui) na crate separada `rts-egui` (como `napi`).
/// Resolve via Registry: `ns::egui::register`. `import { Window } from "rts:egui"`.
/// Ver docs/specs/egui-ui-crate-design.md.
pub use rts_egui as egui;
pub mod globals;
pub use rts_std::atomic;
pub use rts_shared::trace;
pub use rts_shared::bigfloat;
pub use rts_shared::buffer;
pub use rts_shared::collections;
pub use rts_std::crypto;
pub use rts_shared::date;
pub use rts_std::engine;
pub use rts_std::env;
pub use rts_std::events;
pub use rts_std::event_loop;
pub use rts_shared::fmt;
pub use rts_std::ffi;
pub use rts_std::fs;
pub use rts_std::collector;
/// Alias retrocompatível: a pasta `gc/` foi renomeada `collector/` (Fase 2 GC).
/// O collector (gc-surface: string_pool/error/generator/collector/stack) migrou
/// pro `rts-std` (backend do gc); aqui só a fachada. `crate::namespaces::gc::*`
/// no codegen + ~68 consumidores seguem resolvendo via este alias.
pub use collector as gc;
pub use rts_shared::hash;
pub use rts_shared::hint;
pub use rts_std::http_server;
pub use rts_std::io;
pub use rts_shared::json;
pub use rts_shared::math;
pub use rts_shared::mem;
pub use rts_std::net;
pub use rts_shared::num;
pub use rts_std::os;
pub use rts_shared::path;
pub use rts_std::process;
pub use rts_std::promise;
pub use rts_shared::ptr;
pub use rts_shared::regex;
pub use rts_std::parallel;
pub use rts_std::runtime;
// string movido para globals/string
pub use rts_std::sync;
pub use rts_std::test;
pub use rts_std::thread;
pub use rts_std::tls;
pub use rts_std::time;
