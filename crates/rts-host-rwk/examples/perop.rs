//! Is the cost per operator, or per pass?
//!
//! The kernel is slow and the explanation offered for it — that every operator
//! is a runtime call, because nothing has been proved about any operand — is a
//! claim, not a measurement. This falsifies it: two loops with the same number
//! of passes and a different number of operators. If the time tracks the
//! operator count, the calls are the cost. If it tracks the passes, they are
//! not and the explanation is wrong.

use std::time::Instant;

fn time(source: &str, repeats: u32) -> f64 {
    let mut program = rts_host_rwk::compile(source).expect("compiles");
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

    // Three loops over the same number of passes. Each `+` is one more call,
    // and nothing else differs.
    let cases = [
        (
            "3 operators",
            format!(
                "let t = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ t = t + i; }} return t;"
            ),
        ),
        (
            "4 operators",
            format!(
                "let t = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ t = t + i + i; }} return t;"
            ),
        ),
        (
            "5 operators",
            format!(
                "let t = 0; for (let i = 0; i < {rounds}; i = i + 1) {{ t = t + i + i + i; }} return t;"
            ),
        ),
    ];

    println!("rounds {rounds}");
    for (label, source) in &cases {
        let ms = time(source, 5);
        println!(
            "{label:<14} {ms:8.1} ms   {:6.1} ns/pass",
            ms * 1_000_000.0 / rounds as f64
        );
    }
}
