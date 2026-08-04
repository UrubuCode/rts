//! What a property access costs.
//!
//! E4 made property access correct and left it slow on purpose: every `o.x` is
//! a call that looks a key up in a layout. The machine has `cached_get` and
//! `guard_type` for a site that keeps seeing the same shape, and nothing calls
//! them.
//!
//! Whether to write that is a question with a number, and this is the number.
//! The type pass is the precedent: it was worth 24.5 ns per operator, measured
//! before it was written, and the measurement is what made its target specific.

use std::time::Instant;

fn time(source: &str, repeats: u32) -> f64 {
    let program = rts_host_rwk::compile(source).expect("compiles");
    let mut best = f64::INFINITY;
    for _ in 0..repeats {
        let started = Instant::now();
        program.run();
        best = best.min(started.elapsed().as_secs_f64() * 1000.0);
    }
    best
}

fn main() {
    let rounds: i64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(2_000_000);

    // Same loop, same number of passes. The only difference is where the
    // addend comes from: a proven local, or a property.
    let cases = [
        (
            "local (proven)",
            format!("let n = 1; let t = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ t = t + n; }} return t;"),
        ),
        (
            "one property read",
            format!("let o = {{}}; o.n = 1; let t = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ t = t + o.n; }} return t;"),
        ),
        (
            "two property reads",
            format!("let o = {{}}; o.n = 1; o.m = 0; let t = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ t = t + o.n + o.m; }} return t;"),
        ),
        (
            "one property write",
            format!("let o = {{}}; o.n = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ o.n = i; }} return 0;"),
        ),
    ];

    println!("rounds {rounds}");
    let mut baseline = 0.0;
    for (label, source) in &cases {
        let ms = time(source, 5);
        let ns = ms * 1_000_000.0 / rounds as f64;
        if *label == "local (proven)" {
            baseline = ns;
        }
        println!("{label:<20} {ms:8.1} ms   {ns:7.1} ns/pass   {:+7.1} over local", ns - baseline);
    }
}
