//! node:diagnostics_channel — the `extern "C"` entry points: the module
//! functions (`channel`/`hasSubscribers`/`subscribe`/`unsubscribe`) and the
//! `Channel` class members (`publish`, `subscribe`, `hasSubscribers`, `name`).

use super::registry;

unsafe extern "C" {
    fn __RTS_FN_NS_GC_STRING_NEW(ptr: *const u8, len: i64) -> u64;
}

fn read(ptr: *const u8, len: i64) -> String {
    unsafe { rts_engine::abi::str_abi::from_abi(ptr, len) }.unwrap_or("").to_string()
}

fn intern(s: &str) -> u64 {
    unsafe { __RTS_FN_NS_GC_STRING_NEW(s.as_ptr(), s.len() as i64) }
}

// ---- module functions ----

/// `diagnostics_channel.channel(name)` → Channel.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_CHANNEL(p: *const u8, l: i64) -> u64 {
    registry::build_channel(&read(p, l))
}

/// `diagnostics_channel.hasSubscribers(name)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_HAS_SUBSCRIBERS(p: *const u8, l: i64) -> i64 {
    registry::has_subscribers(&read(p, l)) as i64
}

/// `diagnostics_channel.subscribe(name, onMessage)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_SUBSCRIBE(p: *const u8, l: i64, on_message: u64) {
    registry::subscribe(&read(p, l), on_message);
}

/// `diagnostics_channel.unsubscribe(name, onMessage)` → whether one was removed.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_UNSUBSCRIBE(p: *const u8, l: i64, on_message: u64) -> i64 {
    registry::unsubscribe(&read(p, l), on_message) as i64
}

// ---- Channel instance members ----

/// `channel.publish(message)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_PUBLISH(this: u64, message: u64) {
    registry::publish(&registry::channel_name(this), message);
}

/// `channel.subscribe(onMessage)`.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_CHANNEL_SUBSCRIBE(this: u64, on_message: u64) {
    registry::subscribe(&registry::channel_name(this), on_message);
}

/// `channel.hasSubscribers` (getter).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_CHANNEL_HAS_SUBSCRIBERS(this: u64) -> i64 {
    registry::has_subscribers(&registry::channel_name(this)) as i64
}

/// `channel.name` (getter).
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_NODE_DC_CHANNEL_NAME(this: u64) -> u64 {
    intern(&registry::channel_name(this))
}
