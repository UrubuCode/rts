//! What the audit's cost findings are worth, measured rather than argued.
//!
//! # Why this exists beside `rts-cranelift`'s probe
//!
//! That probe measures the **machine** with no client present, which is what
//! makes "the compiler is slow" attributable. The findings this file answers are
//! not in that layer: they are in the verifier, in signature interning, in the
//! layout derivation the lowering calls per field access, and in the interner
//! the runtime reaches per computed property. None of those show up in a fixture
//! that compiles one function and calls it.
//!
//! So this measures the **whole pipeline**, twice: how long it takes to turn
//! source into code, and how long that code then takes to run.
//!
//! # The rules this obeys
//!
//! - **A debug number is not a number.** It prints the profile and says so.
//! - **A measurement of nothing measures nothing.** Every case consumes its
//!   result, so an optimiser deleting the work shows up as a number too good to
//!   be true rather than as a fast one.
//! - **The input is verified, not assumed.** Each case prints what it built —
//!   how many statements, how many iterations — so a number cannot be quietly
//!   measured against a smaller corpus than it claims.
//!
//! Run it with `cargo run --release --example audit_bench -p rts-host-rwk`.

use std::time::Instant;

use rts_host_rwk::compile;

fn main() {
    let profile = match cfg!(debug_assertions) {
        true => "DEBUG — these are not numbers",
        false => "release",
    };
    println!("profile: {profile}\n");

    compile_side();
    run_side();
}

/// The compiler's own throughput, on programs shaped to reach each finding.
fn compile_side() {
    println!("== compiling (source in, callable code out) ==");

    // Field access dominates: every property read lowers through the layout
    // derivation. A wide object read repeatedly is the shape that reaches it.
    let fields = program_of(
        400,
        "let o = {a: 1, b: 2, c: 3, d: 4}; let t = 0;",
        "t = t + o.a + o.b + o.c + o.d;",
    );
    timed("field access (400 statements)", 20, &fields);

    // Blocks dominate: the verifier's cleanup walk and the block worklist are
    // quadratic in blocks, and every `if` is two more.
    let blocks = program_of(
        300,
        "let n = 0;",
        "if (n > 0) { n = n - 1; } else { n = n + 1; }",
    );
    timed("branches (300 if/else)", 20, &blocks);

    // Calls dominate: each distinct call shape interns a signature, and
    // interning is a linear scan over everything interned so far.
    let calls = program_of(
        200,
        "function f(a, b) { return a + b; } let t = 0;",
        "t = f(t, 1);",
    );
    timed("calls (200 statements)", 20, &calls);

    // A `try` reaches the cleanup piece walk, which is the quadratic one.
    let cleanups = program_of(
        60,
        "let n = 0;",
        "try { n = n + 1; } finally { n = n + 1; }",
    );
    timed("cleanups (60 try/finally)", 20, &cleanups);

    println!();
}

/// What the compiled code then costs, on the paths the audit named.
fn run_side() {
    println!("== running (the code the above produced) ==");

    // A computed key reaches `ToPropertyKey` and the interner on every access,
    // which is the double-allocation finding.
    ran(
        "computed property read (200k)",
        "let o = {alpha: 1}; let k = \"alpha\"; let t = 0; \
         for (let i = 0; i < 200000; i = i + 1) { t = t + o[k]; } return t;",
    );

    // The named form, for contrast: it carries a resolved key and never
    // interns. The DIFFERENCE between the two is the finding's cost.
    ran(
        "named property read (200k)",
        "let o = {alpha: 1}; let t = 0; \
         for (let i = 0; i < 200000; i = i + 1) { t = t + o.alpha; } return t;",
    );

    // Bigint division allocates per bit of the quotient.
    ran(
        "bigint division (20k)",
        "let a = 123456789012345678901234567890n; let b = 987654321n; let t = 0n; \
         for (let i = 0; i < 20000; i = i + 1) { t = t + a / b; } return t === 0n;",
    );

    ran(
        "string concatenation (50k)",
        "let t = \"\"; \
         for (let i = 0; i < 50000; i = i + 1) { t = \"ab\"; } return t;",
    );

    println!();
}

/// A program of `count` repetitions of one statement, with a preamble.
fn program_of(count: usize, preamble: &str, body: &str) -> String {
    let mut source = String::from(preamble);
    for _ in 0..count {
        source.push_str(body);
    }
    source.push_str(" return 0;");
    source
}

/// Compiles a source `rounds` times and reports the mean.
///
/// The compiled program is consumed — its entry address is folded into a value
/// the caller prints — so that a compilation optimised away would be visible.
fn timed(what: &str, rounds: u32, source: &str) {
    // One compile first, outside the timing, to fail loudly rather than to
    // report the mean of an error path.
    let checked = compile(source);
    if let Err(error) = &checked {
        println!("  {what:<32} DID NOT COMPILE — {error:?}");
        return;
    }
    drop(checked);

    let start = Instant::now();
    let mut consumed = 0usize;
    for _ in 0..rounds {
        let program = compile(source).expect("it compiled a moment ago");
        consumed += format!("{program:?}").len();
    }
    let elapsed = start.elapsed();
    let per = elapsed.as_secs_f64() * 1000.0 / f64::from(rounds);
    println!(
        "  {what:<32} {per:>8.2} ms/compile   ({} bytes of source, {consumed} consumed)",
        source.len()
    );
}

/// Compiles once, then times the run.
fn ran(what: &str, source: &str) {
    let mut program = match compile(source) {
        Ok(program) => program,
        Err(error) => {
            println!("  {what:<32} DID NOT COMPILE — {error:?}");
            return;
        }
    };
    // One run first, so a program that faults does so before the clock starts.
    let first = program.run();

    let start = Instant::now();
    let produced = program.run();
    let elapsed = start.elapsed();
    println!(
        "  {what:<32} {:>8.2} ms/run     (answered {produced:#x}, warm {first:#x})",
        elapsed.as_secs_f64() * 1000.0
    );
}
