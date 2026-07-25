//! Global JS objects — primitivas JS (String, Number, Boolean, Array, Object, Function, Promise)
//! + JSON, Date, console, globalThis, RegExp, Error family, timers, fetch,
//! TextEncoder/Decoder, atob/btoa, structuredClone, URL, performance.

pub use rts_std::globals::imgdec;
pub mod dataview;
pub use rts_primitives::array;
pub use rts_primitives::boolean;
pub use rts_std::globals::console;
pub use rts_shared::globals::date;
pub use rts_shared::globals::dom_exception;
pub use rts_shared::globals::point;
pub use rts_shared::globals::point3;
pub use rts_primitives::number;
pub use rts_primitives::object;
pub use rts_primitives::promise;
pub use rts_primitives::error;
pub use rts_std::globals::abort;
pub use rts_std::globals::event_target;
pub use rts_std::globals::events;
pub use rts_std::globals::fetch;
pub use rts_std::globals::headers;
pub use rts_primitives::finalization_registry;
pub use rts_primitives::function;
pub use rts_shared::globals::global_this;
pub use rts_shared::globals::json;
pub use rts_shared::globals::json5;
pub use rts_std::globals::performance;
pub use rts_primitives::proxy;
pub use rts_primitives::reflect;
pub use rts_primitives::regexp;
pub use rts_primitives::string;
pub use rts_primitives::symbol;
pub use rts_std::globals::text_encoding;
pub use rts_std::globals::timers;
pub use rts_shared::globals::url;
pub use rts_primitives::weakref;
