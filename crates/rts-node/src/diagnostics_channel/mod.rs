//! `node:diagnostics_channel` — the in-process publish/subscribe bus. Full core
//! surface: the module functions `channel`/`hasSubscribers`/`subscribe`/
//! `unsubscribe` and the `Channel` class (`publish`, `subscribe`,
//! `hasSubscribers` getter, `name` getter). Subscribers are real function values
//! invoked synchronously through the codegen callback bridge — no stubs.
//!
//! `Channel` is an object-backed Registry class (`__rts_class = "Channel"`, the
//! StringDecoder/Hash model); `channel(name)` builds the instance and its
//! `ts_signature` return type `Channel` drives method dispatch.
//!
//! `unsubscribe` matches subscribers by function-value identity (the raw
//! PolyValue FUNCTION word). The logic is correct, but the engine currently
//! reifies each PolyValue function ARGUMENT to a fresh handle at every builtin
//! call-site, so the same `cb` passed to `subscribe` then `unsubscribe` arrives
//! as two different words and does not match — reference removal is therefore
//! not reliable yet (an engine limitation shared with the whole callback-arg
//! path). A follow-up engine change preserving function-arg identity across
//! builtin calls would enable it with no change here.
//!
//! Deferred (need the async-context / AsyncLocalStorage / promise subsystems):
//! `channel.bindStore`/`runStores`, and the whole `TracingChannel` helper
//! (`tracingChannel`, `traceSync`/`tracePromise`/`traceCallback`, the
//! start/end/asyncStart/asyncEnd/error sub-channels).
//!
//! Layout: `registry` (sub store + invoke bridge), `symbols` (extern points),
//! `mod` (registration).

mod registry;
mod symbols;

use rts_engine::AbiType::{self, Bool, Handle, PolyValue, StrPtr, Void};
use rts_engine::{Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

#[allow(clippy::too_many_arguments)]
fn mem(name: &str, kind: MemberKind, args: Vec<AbiType>, ret: AbiType, symbol: &str, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind,
        sig: Sig::new(args, ret),
        symbol: symbol.to_string(),
        fn_ptr: FnPtr(fp),
        flags: MemberFlags::NONE,
        aliases: Vec::new(),
        variadic: false,
        ts_signature: ts.to_string(),
        doc: String::new(),
        pure: false,
        intrinsic: None,
    }
}

/// Registers the `Channel` class + the `node:diagnostics_channel` module.
pub fn register(e: &mut Engine) {
    use symbols as s;
    use MemberKind::{Function, InstanceGetter, InstanceMethod};

    e.class("Channel")
        .doc("Channel — a named diagnostics pub/sub channel (node:diagnostics_channel).")
        .member(mem("publish", InstanceMethod, vec![Handle, PolyValue], Void, "__RTS_FN_NODE_DC_PUBLISH", "publish(message: object): void", s::__RTS_FN_NODE_DC_PUBLISH as *const u8))
        .member(mem("subscribe", InstanceMethod, vec![Handle, PolyValue], Void, "__RTS_FN_NODE_DC_CHANNEL_SUBSCRIBE", "subscribe(onMessage: object): void", s::__RTS_FN_NODE_DC_CHANNEL_SUBSCRIBE as *const u8))
        .member(mem("hasSubscribers", InstanceGetter, vec![Handle], Bool, "__RTS_FN_NODE_DC_CHANNEL_HAS_SUBSCRIBERS", "hasSubscribers: boolean", s::__RTS_FN_NODE_DC_CHANNEL_HAS_SUBSCRIBERS as *const u8))
        .member(mem("name", InstanceGetter, vec![Handle], Handle, "__RTS_FN_NODE_DC_CHANNEL_NAME", "name: string", s::__RTS_FN_NODE_DC_CHANNEL_NAME as *const u8))
        .done();

    e.ns("node:diagnostics_channel")
        .doc("Diagnostics pub/sub (node:diagnostics_channel): channel, hasSubscribers, subscribe, unsubscribe.")
        .member(mem("channel", Function, vec![StrPtr], Handle, "__RTS_FN_NODE_DC_CHANNEL", "channel(name: string): Channel", s::__RTS_FN_NODE_DC_CHANNEL as *const u8))
        .member(mem("hasSubscribers", Function, vec![StrPtr], Bool, "__RTS_FN_NODE_DC_HAS_SUBSCRIBERS", "hasSubscribers(name: string): boolean", s::__RTS_FN_NODE_DC_HAS_SUBSCRIBERS as *const u8))
        .member(mem("subscribe", Function, vec![StrPtr, PolyValue], Void, "__RTS_FN_NODE_DC_SUBSCRIBE", "subscribe(name: string, onMessage: object): void", s::__RTS_FN_NODE_DC_SUBSCRIBE as *const u8))
        .member(mem("unsubscribe", Function, vec![StrPtr, PolyValue], Bool, "__RTS_FN_NODE_DC_UNSUBSCRIBE", "unsubscribe(name: string, onMessage: object): boolean", s::__RTS_FN_NODE_DC_UNSUBSCRIBE as *const u8))
        .done();
}
