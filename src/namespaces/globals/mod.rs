//! Global JS objects — primitivas JS (String, Number, Boolean, Array, Object, Function, Promise)
//! + JSON, Date, console, globalThis, RegExp, Error family, timers, fetch,
//! TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub mod console;
pub mod date;
pub mod number;
pub mod error;
pub mod events;
pub mod fetch;
pub mod function;
pub mod global_this;
pub mod json;
pub mod performance;
pub mod regexp;
pub mod string;
pub mod symbol;
pub mod text_encoding;
pub mod timers;
pub mod url;
