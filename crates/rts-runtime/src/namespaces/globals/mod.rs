//! Global JS objects — primitivas JS (String, Number, Boolean, Array, Object, Function, Promise)
//! + JSON, Date, console, globalThis, RegExp, Error family, timers, fetch,
//! TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub use rts_std::globals::abort;
pub use rts_shared::globals::bigint;
pub use rts_std::globals::blob;
pub mod dataview;
pub use rts_shared::globals::dom_exception;
pub use rts_std::globals::event_target;
pub use rts_std::globals::form_data;
pub use rts_primitives::array;
pub use rts_primitives::boolean;
pub use rts_std::globals::console;
pub use rts_shared::globals::date;
pub use rts_primitives::number;
pub use rts_primitives::promise;
pub use rts_primitives::error;
pub use rts_std::globals::events;
pub use rts_std::globals::fetch;
pub use rts_shared::globals::function;
pub use rts_shared::globals::global_this;
pub use rts_std::globals::headers;
pub use rts_shared::globals::intl;
pub use rts_shared::globals::json;
pub use rts_shared::globals::json5;
pub use rts_std::globals::message_channel;
pub use rts_std::globals::performance;
pub use rts_shared::globals::proxy;
pub use rts_std::globals::readable_stream;
pub use rts_shared::globals::reflect;
pub use rts_shared::globals::regexp;
pub use rts_primitives::string;
pub use rts_shared::globals::symbol;
pub use rts_std::globals::text_encoding;
pub use rts_std::globals::timers;
pub use rts_shared::globals::url;
pub use rts_shared::globals::weakmap;
pub use rts_shared::globals::weakref;
pub use rts_shared::globals::weakset;
pub use rts_shared::globals::finalization_registry;
