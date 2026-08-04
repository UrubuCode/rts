//! Timing one numeric kernel, so it can be compared with another engine.
//!
//! # What this measures, and what it cannot
//!
//! The new engine runs almost nothing of JavaScript. No functions, no objects,
//! no strings — `emit` is at E3. So a comparison is only possible on the
//! intersection of what both engines can do, which is arithmetic over locals in
//! a loop, and a kernel chosen that way says nothing about a program that uses
//! anything else.
//!
//! It also flatters nothing. **Every operator here is a call into the runtime**,
//! because no type pass exists and nothing has been proved about any operand.
//! `i + 1` is a function call. So this is a floor: what the engine costs before
//! any of the work that makes such calls unnecessary.
//!
//! # Why compile and run are timed apart
//!
//! Because they answer different questions and get confused with each other. A
//! host that compiles fast and runs slowly and one that does the reverse are
//! different tools, and a single number hides which one is in front of you.

use std::time::Instant;

/// The kernel, in the subset both engines can run.
///
/// Written so the loop cannot be reasoned away: the total depends on every
/// pass. Neither engine constant-folds today, and if one starts to, this stops
/// measuring what it claims — which is why the bound comes from the command
/// line rather than from a literal that could be folded with it.
fn kernel(rounds: i64) -> String {
    format!(
        "let total = 0; \
         for (let i = 0; i < {rounds}; i = i + 1) {{ total = total + i; }} \
         return total;"
    )
}

fn main() {
    let rounds: i64 = std::env::args()
        .nth(1)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(1_000_000);
    let repeats: u32 = std::env::args()
        .nth(2)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(5);

    let source = kernel(rounds);

    // Compiling is timed on its own, and separately from running, because the
    // two are separate costs a caller pays at separate times.
    let mut compile_times = Vec::new();
    let mut compiled: Option<rts_host_rwk::Compiled> = None;
    for _ in 0..repeats {
        let started = Instant::now();
        let program = rts_host_rwk::compile(&source).expect("the kernel compiles");
        compile_times.push(started.elapsed().as_secs_f64() * 1000.0);
        compiled = Some(program);
    }
    let mut program = compiled.expect("at least one repeat");

    let mut run_times = Vec::new();
    let mut answer = 0u64;
    for _ in 0..repeats {
        let started = Instant::now();
        answer = program.run();
        run_times.push(started.elapsed().as_secs_f64() * 1000.0);
    }

    // The answer is printed, not discarded. A benchmark whose result nobody
    // reads is one an optimiser is entitled to delete, and one whose result is
    // wrong is worse than slow.
    let total = rts_cranelift::tags::decode_double(answer);
    let expected = (rounds as f64 - 1.0) * (rounds as f64) / 2.0;
    assert_eq!(total, expected, "the kernel computed the wrong sum");

    println!("rounds        {rounds}");
    println!("repeats       {repeats}");
    println!("answer        {total}");
    println!("compile ms    {}", summarise(&compile_times));
    println!("run ms        {}", summarise(&run_times));
}

/// Best and median, not mean.
///
/// The best run is the one with the least interference from everything else on
/// the machine, and the median says whether that best was typical. A mean over
/// five samples is dragged around by whichever one the scheduler interrupted.
fn summarise(samples: &[f64]) -> String {
    let mut sorted = samples.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("no NaN in a duration"));
    format!(
        "best {:.3}  median {:.3}",
        sorted[0],
        sorted[sorted.len() / 2]
    )
}
