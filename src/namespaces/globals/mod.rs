//! Global JS objects — `JSON`, `Date`, `console`, `globalThis`, `RegExp`, `Error` family,
//! timers, fetch, TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub mod console;
pub mod date;
pub mod error;
pub mod events;
pub mod fetch;
pub mod global_this;
pub mod json;
pub mod performance;
pub mod regexp;
pub mod text_encoding;
pub mod timers;
pub mod url;
