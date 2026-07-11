//! `node:timers` — `setTimeout`/`setInterval`/`setImmediate` + their `clear*`.
//! These are the SAME primitives the engine already exposes as globals
//! (`__RTS_FN_GL_TIMERS_*`, driven by the real event-loop drain); `node:timers`
//! just registers the module specifier so `import { setTimeout } from
//! "node:timers"` resolves to them — reuse, no duplication (the same approach the
//! other node modules take for engine globals + the `node:util` fmt helpers). The
//! externs live in the engine's timers instance; rts-node declares them so their
//! real addresses become the members' fn_ptr (JIT-harvested under this module).
//!
//! Deferred: the extra `...args` forwarded to the callback (the engine timer
//! calls `callback(0)`), the `Timeout`/`Immediate` OBJECT return (RTS returns a
//! numeric handle — `clearTimeout` accepts it), `ref`/`unref`/`refresh` on that
//! object, and the whole `node:timers/promises` async surface.

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

// The engine timer externs (defined in the engine's timers instance; linked into
// the final binary). Taking their address gives the members a real fn_ptr.
unsafe extern "C" {
    fn __RTS_FN_GL_TIMERS_SET_TIMEOUT(fp: u64, delay_ms: i64) -> u64;
    fn __RTS_FN_GL_TIMERS_CLEAR_TIMEOUT(handle: u64);
    fn __RTS_FN_GL_TIMERS_SET_INTERVAL(fp: u64, interval_ms: i64) -> u64;
    fn __RTS_FN_GL_TIMERS_CLEAR_INTERVAL(handle: u64);
    fn __RTS_FN_GL_TIMERS_SET_IMMEDIATE(fp: u64) -> u64;
    fn __RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE(handle: u64);
}

fn m(name: &str, symbol: &str, sig: Sig, ts: &str, fp: *const u8) -> Member {
    Member {
        name: name.to_string(),
        kind: MemberKind::Function,
        sig,
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

/// Registers the `node:timers` surface (re-export of the engine timer globals).
pub fn register(e: &mut Engine) {
    use AbiType::{Handle, I64, U64, Void};
    e.ns("node:timers")
        .doc("Timers (node:timers): setTimeout/clearTimeout, setInterval/clearInterval, setImmediate/clearImmediate.")
        .member(m("setTimeout", "__RTS_FN_GL_TIMERS_SET_TIMEOUT", Sig::new(vec![U64, I64], Handle), "setTimeout(callback: () => void, ms: number): number", __RTS_FN_GL_TIMERS_SET_TIMEOUT as *const u8))
        .member(m("clearTimeout", "__RTS_FN_GL_TIMERS_CLEAR_TIMEOUT", Sig::new(vec![Handle], Void), "clearTimeout(handle: number): void", __RTS_FN_GL_TIMERS_CLEAR_TIMEOUT as *const u8))
        .member(m("setInterval", "__RTS_FN_GL_TIMERS_SET_INTERVAL", Sig::new(vec![U64, I64], Handle), "setInterval(callback: () => void, ms: number): number", __RTS_FN_GL_TIMERS_SET_INTERVAL as *const u8))
        .member(m("clearInterval", "__RTS_FN_GL_TIMERS_CLEAR_INTERVAL", Sig::new(vec![Handle], Void), "clearInterval(handle: number): void", __RTS_FN_GL_TIMERS_CLEAR_INTERVAL as *const u8))
        .member(m("setImmediate", "__RTS_FN_GL_TIMERS_SET_IMMEDIATE", Sig::new(vec![U64], Handle), "setImmediate(callback: () => void): number", __RTS_FN_GL_TIMERS_SET_IMMEDIATE as *const u8))
        .member(m("clearImmediate", "__RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE", Sig::new(vec![Handle], Void), "clearImmediate(handle: number): void", __RTS_FN_GL_TIMERS_CLEAR_IMMEDIATE as *const u8))
        .done();
}
