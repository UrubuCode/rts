//! `performance` — `performance.now()` / `performance.timeOrigin`.
//!
//! Authored with `#[rtse::function]`: each function declares its module and JS
//! name, and everything else is DERIVED from the Rust signature — the linker
//! symbol (`rts_abi::scope`), the `AbiType`s, the TS signature, and the doc.
//! The old form spelled all of that a second time in a hand-built `Member`
//! literal that nothing checked against the function it claimed to describe.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rts_engine::Engine;

static START: OnceLock<(Instant, f64)> = OnceLock::new();

fn start() -> &'static (Instant, f64) {
    START.get_or_init(|| {
        let origin_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64()
            * 1000.0;
        (Instant::now(), origin_ms)
    })
}

/// performance.now() — tempo monotônico em milissegundos (precisão sub-ms).
#[rtse::function(module = "performance", value = "now")]
fn now() -> f64 {
    let (inst, _) = start();
    inst.elapsed().as_secs_f64() * 1000.0
}

/// performance.timeOrigin — Unix timestamp em ms do início do processo.
///
/// `constant` (a PROPERTY read, not a call) and `pure` — both were carried by
/// the old `Member` literal and both are load-bearing: without `constant` the
/// name reads as a function value instead of a number.
#[rtse::function(module = "performance", value = "timeOrigin", constant, pure)]
fn time_origin() -> f64 {
    start().1
}

/// Registra a namespace `performance` no motor.
pub fn register(e: &mut Engine) {
    e.module("performance", |m| {
        m.doc("performance.now() / performance.timeOrigin — alias de time.now_ms com precisão sub-ms.");
        m.registry(now_entry());
        m.registry(time_origin_entry());
    });
}
