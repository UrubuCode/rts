//! Global JS objects — `JSON`, `Date`, `console`, `globalThis`.
//!
//! Each sub-module corresponds to one built-in global that is available
//! without an explicit import in TypeScript/JavaScript code.
//!
//! - `json/`       — `JSON.parse` / `JSON.stringify` (aliases existing symbols)
//! - `date/`       — `Date` class: constructor, instance methods, static methods
//! - `console/`    — `console.log`, `console.error`, … (variadic, codegen-special)
//! - `global_this/`— `globalThis`, `global`, `self` aliases

pub mod console;
pub mod date;
pub mod global_this;
pub mod json;
