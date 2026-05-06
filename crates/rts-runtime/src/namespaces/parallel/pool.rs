//! Global Rayon thread pool, lazily initialised.
//!
//! Tamanho do pool: `RTS_THREADS` env var, ou `available_parallelism()`,
//! ou `1` como fallback final. Antes era hardcoded `4` — arbitrario.

use std::sync::OnceLock;

static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

fn desired_threads() -> usize {
    if let Ok(s) = std::env::var("RTS_THREADS") {
        if let Ok(n) = s.parse::<usize>() {
            if n >= 1 {
                return n;
            }
        }
    }
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

pub fn pool() -> &'static rayon::ThreadPool {
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(desired_threads())
            .build()
            .expect("rayon pool init failed")
    })
}
