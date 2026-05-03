//! Global JS objects — primitivas JS (String, Number, Boolean, Array, Object, Function, Promise)
//! + JSON, Date, console, globalThis, RegExp, Error family, timers, fetch,
//! TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub mod array;
pub mod console;
pub mod date;
pub mod number;
pub mod error;
pub mod events;
pub mod fetch;
pub mod function;
pub mod global_this;
pub mod json;
pub mod object;
pub mod performance;
pub mod regexp;
pub mod string;
pub mod symbol;
pub mod text_encoding;
pub mod timers;
pub mod url;
pub mod weakmap;
pub mod weakset;

// Re-exporta os GlobalClassSpec para registro no ABI
pub use array::abi::ARRAY_CLASS_SPEC_FINAL as ARRAY_CLASS_SPEC;
pub use object::abi::OBJECT_CLASS_SPEC;
pub use function::abi::FUNCTION_CLASS_SPEC;
pub use symbol::abi::SYMBOL_CLASS_SPEC;
