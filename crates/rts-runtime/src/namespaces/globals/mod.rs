//! Global JS objects — primitivas JS (String, Number, Boolean, Array, Object, Function, Promise)
//! + JSON, Date, console, globalThis, RegExp, Error family, timers, fetch,
//! TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub mod abort;
pub mod bigint;
pub mod dom_exception;
pub mod event_target;
pub mod form_data;
pub mod boolean;
pub mod console;
pub mod date;
pub mod number;
pub mod error;
pub mod events;
pub mod fetch;
pub mod function;
pub mod global_this;
pub mod headers;
pub mod json;
pub mod json5;
pub mod performance;
pub mod proxy;
pub mod reflect;
pub mod regexp;
pub mod string;
pub mod symbol;
pub mod text_encoding;
pub mod timers;
pub mod url;
pub mod weakmap;
pub mod weakref;
pub mod weakset;
pub mod finalization_registry;
