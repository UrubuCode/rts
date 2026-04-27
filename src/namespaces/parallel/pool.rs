//! Global Rayon thread pool, lazily initialised.

use std::sync::OnceLock;

static POOL: OnceLock<rayon::ThreadPool> = OnceLock::new();

pub fn pool() -> &'static rayon::ThreadPool {
    POOL.get_or_init(|| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4),
            )
            .build()
            .expect("rayon pool init failed")
    })
}
