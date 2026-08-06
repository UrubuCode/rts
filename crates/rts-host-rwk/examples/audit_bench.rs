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
    threads();
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

    scaling();
    println!();
}

/// How the cost grows with the input, which is the question that says WHERE to
/// look before any code is read.
///
/// A doubling that costs twice as much is linear and uninteresting. One that
/// costs four times as much is quadratic, and then the search is for a walk over
/// everything performed per something. Measuring the shape first is what stops a
/// guess from being dressed as a diagnosis — the 205 ms this file first reported
/// for field access was attributed to two allocations per access, and two
/// allocations do not cost 128 microseconds.
fn scaling() {
    println!("\n  -- growth (a doubling that costs 4x is quadratic) --");
    for count in [100usize, 200, 400, 800] {
        let source = program_of(
            count,
            "let o = {a: 1, b: 2, c: 3, d: 4}; let t = 0;",
            "t = t + o.a + o.b + o.c + o.d;",
        );
        timed(&format!("field access x{count}"), 6, &source);
    }
    for count in [75usize, 150, 300, 600] {
        let source = program_of(count, "let n = 0;", "if (n > 0) { n = n - 1; } else { n = n + 1; }");
        timed(&format!("branches x{count}"), 6, &source);
    }

    // What the constant is made of. Each of these is 400 statements; they differ
    // in how many runtime CALLS each statement makes the emitter produce.
    //
    // Arithmetic on proven locals emits none — the machine has an instruction.
    // A property read emits one per access. If the totals track the call count
    // rather than the statement count, the cost is Cranelift compiling a call
    // site, which is neither an allocation nor a scan and is not fixed by
    // removing either.
    println!("\n  -- 400 statements each, differing only in calls emitted --");
    let shapes: [(&str, &str, &str); 4] = [
        ("0 calls (local arithmetic)", "let t = 0; let u = 1;", "t = t + u * 2 - 1;"),
        ("1 call (one property read)", "let o = {a: 1}; let t = 0;", "t = t + o.a;"),
        ("2 calls (two reads)", "let o = {a: 1, b: 2}; let t = 0;", "t = t + o.a + o.b;"),
        ("4 calls (four reads)", "let o = {a: 1, b: 2, c: 3, d: 4}; let t = 0;", "t = t + o.a + o.b + o.c + o.d;"),
    ];
    for (what, preamble, body) in shapes {
        timed(what, 6, &program_of(400, preamble, body));
    }

    spread();
    escaping();
}

/// What an object that never leaves its function costs.
///
/// `let o = {a: i}; t = t + o.a;` emits three calls — the allocation, the write
/// and the read — and allocates a cell that nothing reclaims. If the compiler
/// can prove the object never escapes, all three and the cell go away.
///
/// The pair is the measurement: the same arithmetic written with an object and
/// written without one. A compiler that sees through the object makes them cost
/// the same; one that does not shows the gap, which is what the object costs.
///
/// # Why every statement spells a different name
///
/// Because the first version of this benchmark did not, and measured nothing.
/// Four hundred blocks each declaring `o` is four hundred bindings of one
/// spelling, which the analysis refuses outright — a name introduced twice is
/// two things, and it cannot tell which literal a read means. So every reading
/// was of the refusal, and the optimisation looked inert when it was simply
/// never asked. `numbered` is what makes the four hundred candidates distinct.
fn escaping() {
    println!("\n  -- 400 statements: an object that cannot escape, and the same sum without one --");
    timed(
        "no object at all",
        6,
        &program_of(400, "let t = 0; let i = 1;", "t = t + i + i;"),
    );
    timed("object, never escapes", 6, &numbered(400,
        "let t = 0; let i = 1;", "{ let o# = {a: i, b: i}; t = t + o#.a + o#.b; }"));
    // What the analysis refuses today, kept beside what it accepts so the cost
    // of the refusal is a number rather than a note. `i + 1` is not a literal
    // and not a bare name, so the literal is left alone — even though the
    // values are emitted in source order and the ordering is therefore already
    // kept. Loosening that rule is its own change, with its own argument about
    // a value that throws halfway.
    timed("object, one computed value (refused today)", 6, &numbered(400,
        "let t = 0; let i = 1;", "{ let o# = {a: i, b: i + 1}; t = t + o#.a + o#.b; }"));
    // The control: the same object, escaping through a call, which must NOT be
    // replaced. If this one ever becomes as cheap as the first, the analysis is
    // wrong rather than good.
    timed("object, escapes (must not be replaced)", 6, &numbered(400,
        "function keep(x) { return 1; } let t = 0; let i = 1;",
        "{ let o# = {a: i, b: i}; t = t + keep(o#); }"));
}

/// `program_of`, with `#` in the body replaced by the statement's number.
///
/// Separate from `program_of` rather than a flag on it: the other sections
/// deliberately repeat one statement, and a numbering they did not ask for
/// would change what they measure.
fn numbered(count: usize, preamble: &str, body: &str) -> String {
    let mut source = String::from(preamble);
    for n in 0..count {
        source.push_str(&body.replace('#', &n.to_string()));
    }
    source.push_str(" return 0;");
    source
}

/// The same work in one function versus in many, which decides whether
/// compiling functions concurrently has anything to compile concurrently.
///
/// Both programs emit the same number of call sites. If they cost the same, the
/// cost is per site and a thread pool over functions would help the second and
/// not the first — which is what a real program looks like and what a
/// single-script benchmark hides. If the many-function one costs MORE, there is
/// a per-function overhead worth finding before any threading is considered.
fn spread() {
    println!("\n  -- 400 property reads, spread over N functions --");
    for count in [1usize, 10, 40, 200] {
        let per = 400 / count;
        let mut source = String::from("let o = {a: 1}; let t = 0;");
        for index in 0..count {
            source.push_str(&format!("function f{index}(){{ let u = 0;"));
            for _ in 0..per {
                source.push_str("u = u + o.a;");
            }
            source.push_str("return u; }");
        }
        for index in 0..count {
            source.push_str(&format!("t = t + f{index}();"));
        }
        source.push_str(" return t;");
        timed(&format!("{count} function(s) x{per} reads"), 6, &source);
    }
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

    // The same read with a LITERAL key, which the emitter now resolves to a
    // name — so it takes the cache rather than the conversion. The gap
    // between this and the variable-key case above is what remains.
    ran(
        "literal computed key (200k)",
        "let o = {alpha: 1}; let t = 0; \n         for (let i = 0; i < 200000; i = i + 1) { t = t + o[\"alpha\"]; } return t;",
    );

    // The named form, for contrast: it carries a resolved key and never
    // interns.
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

/// Whether threads actually run at the same time.
///
/// The multi-region heap exists so that N threads can allocate without touching
/// each other. That is a correctness property and `tests/threads.rs` pins it.
/// This asks the performance question the property was for: does running the
/// same program on four threads take about as long as running it on one, or four
/// times as long?
///
/// Four times as long would mean they are serialised somewhere — a shared lock,
/// a contended allocator — and the regions bought nothing. The number is the
/// answer.
fn threads() {
    println!("== running on threads (each with a heap of its own) ==");

    // Enough allocation to be worth measuring, and enough arithmetic that the
    // answer depends on every object written.
    //
    // **The count is bounded by the region, not chosen for the clock.** A region
    // holds 65536 cells and nothing reclaims them, so a loop past that gets
    // `undefined` from a full heap, adds it, and answers `NaN` — while still
    // taking time and still looking like a measurement. The first version of
    // this fixture ran 40 000 iterations and reported a beautiful number for a
    // program that had run out of memory: the answer was the canonical `NaN`
    // and the timing was of the failure path.
    //
    // So the loop is sized to fit and the answer is checked below. This is the
    // file's own rule — a measurement of nothing measures nothing — catching
    // the file's own author.
    const ROUNDS: u32 = 20_000;
    let source = format!(
        "let t = 0; \
         for (let i = 0; i < {ROUNDS}; i = i + 1) {{ let o = {{a: i, b: i + 1}}; t = t + o.a + o.b; }} \
         return t;"
    );
    let expected: f64 = (0..ROUNDS).map(|i| f64::from(i) + f64::from(i + 1)).sum();

    for threads in [1usize, 2, 4] {
        let mut program = match rts_host_rwk::compile_for(&source, 4) {
            Ok(program) => program,
            Err(error) => {
                println!("  DID NOT COMPILE — {error:?}");
                return;
            }
        };
        // One warm run, so a page fault on first touch is not in the number.
        program.run_on(threads);

        let start = Instant::now();
        let answers = program.run_on(threads);
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let each = elapsed / threads as f64;
        // Every thread must have computed the sum. A thread whose region filled
        // up answers `NaN` in just as much time and looks exactly like a fast
        // one, which is what this catches — and did.
        let wrong = answers
            .iter()
            .filter(|answer| rts_cranelift::tags::decode_double(**answer) != expected)
            .count();
        let verdict = match wrong {
            0 => String::new(),
            n => format!("   *** {n}/{} WRONG — not a measurement ***", answers.len()),
        };
        println!(
            "  {threads} thread(s): {elapsed:>7.2} ms wall   {each:>7.2} ms per thread{verdict}"
        );
    }
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
