//! Microbench comparando os 3 caminhos de chamada para `crypto.sha256`:
//!
//! 1. **Boxed dispatch** — `__rts_dispatch(FN_CRYPTO_SHA256, handle, ...)` —
//!    caminho atual do codegen: arg ja esta num handle, `match fn_id` roteia,
//!    `read_runtime_value` desboxa, resultado vai para ValueStore.
//! 2. **Zero-overhead extern C** — `__rts_crypto_sha256_direct(ptr, len)` —
//!    caminho proposto para tipos conhecidos em compile-time (TS `data: str`).
//!    Sem match, sem desboxing do input.
//! 3. **Baseline Rust** — `hash_sha256(&str)` — custo puro do SHA-256.
//!
//! Rode com:
//!   cargo test --release --lib bench_dispatch -- --ignored --nocapture

use std::time::Instant;

use crate::namespaces::abi::{
    FN_CRYPTO_SHA256, __rts_dispatch, push_runtime_value, reset_thread_state,
};
use crate::namespaces::crypto::{__rts_crypto_sha256_direct, hash_sha256};
use crate::namespaces::value::RuntimeValue;

// Payload ~512B — suficiente para SHA-256 fazer trabalho real sem dominar
// totalmente o custo da chamada (queremos o overhead de dispatch visivel).
const INPUT: &str = "rts dispatch benchmark payload rts dispatch benchmark \
     payload rts dispatch benchmark payload rts dispatch benchmark payload \
     rts dispatch benchmark payload rts dispatch benchmark payload rts \
     dispatch benchmark payload rts dispatch benchmark payload rts dispatch \
     benchmark payload rts dispatch benchmark payload rts dispatch benchmark \
     payload rts dispatch benchmark payload rts dispatch benchmark payload";

const ITERS: u64 = 200_000;
const WARMUP: u64 = 20_000;

fn bench_boxed(iters: u64, handle: i64) -> u128 {
    let t = Instant::now();
    let mut acc: i64 = 0;
    for _ in 0..iters {
        acc = acc.wrapping_add(__rts_dispatch(FN_CRYPTO_SHA256, handle, 0, 0, 0, 0, 0));
    }
    std::hint::black_box(acc);
    t.elapsed().as_nanos()
}

fn bench_direct(iters: u64, bytes: &[u8]) -> u128 {
    let ptr = bytes.as_ptr();
    let len = bytes.len() as i64;
    let t = Instant::now();
    let mut acc: i64 = 0;
    for _ in 0..iters {
        acc = acc.wrapping_add(__rts_crypto_sha256_direct(ptr, len));
    }
    std::hint::black_box(acc);
    t.elapsed().as_nanos()
}

fn bench_baseline(iters: u64, s: &str) -> u128 {
    let t = Instant::now();
    let mut acc: usize = 0;
    for _ in 0..iters {
        acc = acc.wrapping_add(hash_sha256(s).len());
    }
    std::hint::black_box(acc);
    t.elapsed().as_nanos()
}

#[test]
#[ignore = "microbench — roda explicitamente com --ignored --nocapture"]
fn bench_dispatch_sha256() {
    let input = INPUT;
    let handle = push_runtime_value(RuntimeValue::String(input.to_string()));

    // Warmup aquece cache + branch predictor.
    bench_boxed(WARMUP, handle);
    bench_direct(WARMUP, input.as_bytes());
    bench_baseline(WARMUP, input);

    // Reset periodico para nao exaurir ValueStore entre rounds.
    reset_thread_state();
    let handle = push_runtime_value(RuntimeValue::String(input.to_string()));

    let boxed_ns = bench_boxed(ITERS, handle);
    reset_thread_state();
    let direct_ns = bench_direct(ITERS, input.as_bytes());
    reset_thread_state();
    let baseline_ns = bench_baseline(ITERS, input);

    let boxed_per = boxed_ns as f64 / ITERS as f64;
    let direct_per = direct_ns as f64 / ITERS as f64;
    let baseline_per = baseline_ns as f64 / ITERS as f64;

    println!();
    println!(
        "=== dispatch overhead — crypto.sha256 ({} iters, payload {}B)",
        ITERS,
        input.len()
    );
    println!("path                        ns/call    vs base    delta vs base");
    println!("------------------------------------------------------------------");
    println!(
        "1. boxed __rts_dispatch     {:>8.1}   {:>6.2}x    {:>+6.1}ns",
        boxed_per,
        boxed_per / baseline_per,
        boxed_per - baseline_per
    );
    println!(
        "2. direct extern C          {:>8.1}   {:>6.2}x    {:>+6.1}ns",
        direct_per,
        direct_per / baseline_per,
        direct_per - baseline_per
    );
    println!(
        "3. Rust baseline            {:>8.1}   {:>6.2}x    {:>+6.1}ns",
        baseline_per, 1.0_f64, 0.0_f64
    );
    println!();
    println!(
        "dispatch overhead (boxed - direct): {:+.1}ns/call",
        boxed_per - direct_per
    );
    println!("speedup direct vs boxed: {:.2}x", boxed_per / direct_per);
}
