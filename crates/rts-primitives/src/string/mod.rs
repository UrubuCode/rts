//! String — primordial `String` value-class + regex-backed method helpers.
//!
//! The old `rts:string` NAMESPACE (`__RTS_FN_NS_STRING_*` free functions:
//! contains/startsWith/toUpper/trim/replace/split/charAt/…) was DRAINED: its whole
//! surface is now computed by the primordial `String` value-class ([`value_class`],
//! pure logic in [`strops`]), so no `__RTS_FN_NS_STRING_*` extern remains and there
//! is no `e.ns("string")` registration.
//!
//! What lives here now:
//!   - [`value_class`] — `#[rtse::class("String", value)]`, the whole JS surface.
//!   - [`strops`] — the pure-Rust ops the value-class computes with.
//!   - [`search`] — regex-backed `match`/`matchAll`/`search` helpers + the
//!     `__rtsadp_str_*_auto` string-vs-regex dispatch trampolines (codegen-owned).
//!   - [`rt`] — the shared string-pool `alloc_str` helper.

pub mod rt;
pub mod search;
pub mod strops;
pub mod value_class;

use rts_engine::Engine;

/// Registra a classe global `String` no motor.
///
/// A superfície JS da classe é AUTORADA em Rust puro, FULLY SELF-CONTAINED, via
/// `#[rtse::class("String", value)]` em [`value_class`] (lógica pura em [`strops`]).
/// A `rts-macro` gera TODO símbolo ABI; os corpos computam tudo em Rust — SEM
/// `.member(...)` à mão. Esta fn é um wrapper FINO sobre o `register` gerado pela
/// macro (mantém o NOME: o codegen reroteia pra cá).
///
/// EXCLUÍDOS da value-class (arg[0] é RegExp): `match`/`matchAll`/`search`/`split`,
/// cujo dispatch string-vs-regex vive nos trampolins `__rtsadp_str_*_auto`
/// ([`search`]).
pub fn register_string_class_spec(e: &mut Engine) {
    value_class::register(e);
}
