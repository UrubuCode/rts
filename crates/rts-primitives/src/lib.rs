//! rts-primitives — classes PRIMORDIAIS da linguagem (o único conjunto que o
//! motor/codegen pode citar diretamente). Tudo o mais resolve dinamicamente pelo
//! Registry. Conjunto: String, Object, Array, Function, Promise, Boolean, Number,
//! Error (+ TypeError/RangeError/ReferenceError/SyntaxError/URIError/EvalError/
//! AggregateError).
//!
//! Fase 2 (extração incremental): os módulos primordiais migram de `rts-shared`
//! p/ cá um a um, com gate de build+suíte a cada passo. Crate só depende de
//! `rts-engine` (universal/wasm-safe).

pub mod gc_surface;

/// Embedded TypeScript source of the PRIMORDIAL `Error` family (Error +
/// TypeError/RangeError/ReferenceError/SyntaxError/URIError/EvalError/
/// AggregateError). The new engine `include`s this as a declarations-only
/// prelude: the user's `new Error("x")` constructs this `.ts` class (a shape-
/// based object), `.message`/`.name`/`.stack` are ordinary slots, `.stack` is a
/// REAL `engine.trace_capture()` trace, `toString()` is the `.ts` method, and
/// `instanceof` rides the normal user-class inheritance chain. This replaces the
/// former hardcoded codegen synth + `__rtsadp_err_*` trampolines.
///
/// Must be concatenated BEFORE the Map/Set stdlib so the error SUBCLASSES (which
/// `extends Error`) see the `Error` base declared first (one merged prelude
/// program; declaration order within the include string matters).
pub const ERROR_TS: &str = include_str!("error.ts");

pub mod array;
pub mod boolean;
pub mod error;
pub mod function;
pub mod number;
pub mod object;
pub mod promise;
pub mod string;
