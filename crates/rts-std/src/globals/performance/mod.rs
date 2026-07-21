//! `performance` — performance.now() / performance.timeOrigin. Migrado do
//! `#[rts_namespace]` pro modelo builder hand-written do `rts-engine` (rumo à
//! remoção da `rts-macro`); stem de símbolo `GL_PERF` (escopo GL). Os externs
//! `now`/`timeOrigin` são `#[no_mangle]` à mão; a SPEC é registrada via
//! [`register`].

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rts_engine::{AbiType, Engine, FnPtr, Member, MemberFlags, MemberKind, Sig};

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
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PERF_NOW() -> f64 {
    let (inst, _) = start();
    inst.elapsed().as_secs_f64() * 1000.0
}

/// performance.timeOrigin — Unix timestamp em ms do início do processo.
#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PERF_TIME_ORIGIN() -> f64 {
    start().1
}

/// Registra a namespace `performance` no motor (hand-written, sem macro).
pub fn register(e: &mut Engine) {
    e.ns("performance")
        .doc("performance.now() / performance.timeOrigin — alias de time.now_ms com precisão sub-ms.")
        .member(Member {
            name: "now".to_string(),
            kind: MemberKind::Function,
            sig: Sig::new(Vec::new(), AbiType::F64),
            symbol: "__RTS_FN_GL_PERF_NOW".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_PERF_NOW as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "now(): number".to_string(),
            doc: "performance.now() — tempo monotônico em milissegundos (precisão sub-ms)."
                .to_string(),
            pure: false,
            emit: None,
        })
        .member(Member {
            name: "timeOrigin".to_string(),
            kind: MemberKind::Constant,
            sig: Sig::new(Vec::new(), AbiType::F64),
            symbol: "__RTS_FN_GL_PERF_TIME_ORIGIN".to_string(),
            fn_ptr: FnPtr(__RTS_FN_GL_PERF_TIME_ORIGIN as *const u8),
            flags: MemberFlags::NONE,
            aliases: Vec::new(),
            variadic: false,
            ts_signature: "timeOrigin: number".to_string(),
            doc: "performance.timeOrigin — Unix timestamp em ms do início do processo."
                .to_string(),
            pure: true,
            emit: None,
        })
        .done();
}
