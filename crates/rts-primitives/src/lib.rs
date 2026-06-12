//! rts-primitives — classes PRIMORDIAIS da linguagem (o único conjunto que o
//! motor/codegen pode citar diretamente). Tudo o mais resolve dinamicamente pelo
//! Registry. Conjunto: String, Object, Array, Function, Promise, Boolean, Number,
//! Error (+ TypeError/RangeError/ReferenceError/SyntaxError/URIError/EvalError/
//! AggregateError).
//!
//! Fase 2 (extração incremental): os módulos primordiais migram de `rts-shared`
//! p/ cá um a um, com gate de build+suíte a cada passo. Crate só depende de
//! `rts-engine` (universal/wasm-safe).

pub mod boolean;
pub mod number;
