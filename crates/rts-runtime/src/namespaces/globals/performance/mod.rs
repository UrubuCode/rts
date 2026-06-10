//! `performance` — performance.now() / performance.timeOrigin. Migrado ao modelo
//! `#[rts_namespace]` (stage 2c) com stem de simbolo `GL_PERF` (escopo GL).

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rts_engine::abi::ty::F64;
use rts_macro::rts_namespace;

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

/// performance.now() / performance.timeOrigin — alias de time.now_ms com precisão sub-ms.
#[rts_namespace(performance, sym = "GL_PERF")]
impl PerformanceNs {
    /// performance.now() — tempo monotônico em milissegundos (precisão sub-ms).
    #[rts_fn]
    pub fn now() -> F64 {
        let (inst, _) = start();
        inst.elapsed().as_secs_f64() * 1000.0
    }

    /// performance.timeOrigin — Unix timestamp em ms do início do processo.
    #[rts_const(name = "timeOrigin", ts = "timeOrigin: number", pure)]
    pub fn time_origin() -> F64 {
        start().1
    }
}
