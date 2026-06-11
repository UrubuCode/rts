//! Global JS objects — primitivas JS (String, Number, Boolean, Array, Object, Function, Promise)
//! + JSON, Date, console, globalThis, RegExp, Error family, timers, fetch,
//! TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub mod abort;
pub use rts_shared::globals::bigint;
pub mod blob;
pub mod dataview;
pub mod dom_exception;
pub mod event_target;
pub mod form_data;
pub use rts_shared::globals::boolean;
pub mod console;
pub mod date;
pub use rts_shared::globals::number;
pub mod error;
pub mod events;
pub mod fetch;
pub mod function;
pub mod global_this;
pub mod headers;
pub mod intl;
pub mod json;
pub mod json5;
pub mod message_channel;
pub mod performance;
pub mod proxy;
pub mod readable_stream;
pub mod reflect;
pub mod regexp;
pub mod string;
pub use rts_shared::globals::symbol;
pub mod text_encoding;
pub mod timers;
pub use rts_shared::globals::url;
pub use rts_shared::globals::weakmap;
pub use rts_shared::globals::weakref;
pub use rts_shared::globals::weakset;
pub use rts_shared::globals::finalization_registry;
