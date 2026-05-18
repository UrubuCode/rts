//! `AbortController` / `AbortSignal` global classes (#62).
//!
//! Versao basica: AbortController.signal retorna signal handle persistente
//! (mesmo handle a cada chamada). AbortController.abort(reason) marca o
//! signal como aborted e dispara os listeners "abort". AbortSignal.addEventListener
//! armazena fn handles que sao invocados em abort.
//!
//! Pendente: AbortSignal.timeout/any/reason wrapping em DOMException.

pub mod abi;
pub mod instance;
