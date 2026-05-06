use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PERF_NOW() -> f64 {
    let (inst, _) = start();
    inst.elapsed().as_secs_f64() * 1000.0
}

#[unsafe(no_mangle)]
pub extern "C" fn __RTS_FN_GL_PERF_TIME_ORIGIN() -> f64 {
    start().1
}
