//! What share of property accesses the inline caches actually answer.
//!
//! # Why the denominator comes from the program and not from a counter
//!
//! A cache HIT never reaches the runtime: the generated code compares the
//! receiver's type number against the cell and loads at the remembered offset,
//! and nothing on that path calls anything. So there is no hit counter to read,
//! and `Context::resolves` — the one number that exists — counts only the
//! misses. `rts-core`'s `entry/mod.rs` says as much where the field is declared:
//! it separates "the cache works" from "the cache is a slower way of calling",
//! and on its own it is a numerator with nothing under it.
//!
//! Adding a hit counter was the obvious alternative and is wrong twice over. It
//! puts an increment in the hottest path this engine has, so it would slow down
//! the thing being measured; and it would have to be compiled in, which means
//! the build that is measured is not the build that ships.
//!
//! So the denominator is taken from the SOURCE instead. Every program below
//! loops a literal number of times over one access, so the count is known by
//! construction, exactly, at zero run-time cost. A cache that works reports a
//! miss count that does not scale with the loop; one that thrashes reports one
//! that does. That distinction is the whole point of the file, and it needs no
//! instrument.
//!
//! # What these tests are NOT
//!
//! They are not timings. A miss costs a call and a by-name resolution while a
//! hit costs two instructions, but nothing here measures either — a rate is not
//! a duration, and reading one as the other is how a share gets quoted as a
//! speedup. What they pin is how often the slow path is taken.

use rts_host::compile;

/// Accesses each program below performs. Large enough that a per-iteration miss
/// is unmistakable against the fixed cost of setting a site up, small enough
/// that the whole file stays well under a second.
const ACCESSES: u64 = 100_000;

/// Runs `source` and answers how many cached reads had to ask the runtime.
///
/// What comes back is the miss count. Callers turn it into a rate themselves,
/// so that each assertion reads in the units its own question is asked in.
fn misses(source: &str) -> u64 {
    let mut program = compile(source).unwrap_or_else(|error| panic!("compiling failed: {error:?}"));
    program.run();
    program.resolves()
}

/// The share of accesses answered without asking the runtime, in percent.
fn hit_rate(missed: u64) -> f64 {
    let hits = ACCESSES.saturating_sub(missed.min(ACCESSES));
    (hits as f64) * 100.0 / (ACCESSES as f64)
}

/// A site that sees one layout forever asks once and then never again.
///
/// The object comes out of a function rather than out of a literal on purpose.
/// Written as `const o = { a: 1 };` the emitter folds the read away entirely —
/// `rts ir` shows ZERO `CachedGet` for that program — so the zero it reports is
/// the absence of an access, not a cache that worked. That version of this test
/// passed while measuring nothing, which is exactly the failure the honesty
/// floor names: verify the input, not just the output.
#[test]
fn a_monomorphic_site_stops_asking_after_the_first_access() {
    let source = "
        function make(v) { return { a: v, b: 2 }; }
        const o = make(1);
        let s = 0;
        for (let i = 0; i < 100000; i++) s += o.a;
        s;
    ";
    let missed = misses(source);
    // Not zero: the site has to learn the layout once, and the program's own
    // setup reads a handful of properties before the loop. What is asserted is
    // that the count does not SCALE — a per-iteration miss would be 100 000.
    assert!(
        missed < 100,
        "a monomorphic site missed {missed} times over {ACCESSES} accesses \
         ({:.4}% hit) — it is re-resolving rather than remembering",
        hit_rate(missed)
    );
}

/// Two receivers of the SAME layout are one layout as far as a site is
/// concerned, and this is the control for the polymorphic test below.
///
/// Without it that test's result is ambiguous: it reaches its receivers through
/// `xs[i & 1]`, so a per-iteration miss could be the indexing rather than the
/// layout. This program indexes identically and differs only in that both
/// objects are built the same way.
#[test]
fn two_receivers_of_one_layout_do_not_make_a_site_polymorphic() {
    let source = "
        function make(v) { return { a: v, b: 2 }; }
        const xs = [make(1), make(3)];
        let s = 0;
        for (let i = 0; i < 100000; i++) s += xs[i & 1].a;
        s;
    ";
    let missed = misses(source);
    assert!(
        missed < 100,
        "two same-layout receivers missed {missed} times over {ACCESSES} \
         accesses ({:.4}% hit) — indexing, not layout, is costing the miss",
        hit_rate(missed)
    );
}

/// A name captured from an enclosing function lives in an environment object,
/// so reading one is a cached property access like any other — and the width of
/// that environment is what `INLINE_SLOTS` decides.
///
/// Twenty bindings is past the fifteen `heap::region` documents, which is the
/// cliff `bench/analytic.ts` warns is still there. It is asserted rather than
/// assumed: at the time of writing, this program does NOT fall off it.
#[test]
fn a_captured_name_is_cached_even_when_its_environment_is_wide() {
    let source = "
        function outer() {
          let a1=1,a2=2,a3=3,a4=4,a5=5,a6=6,a7=7,a8=8,a9=9,a10=10;
          let a11=11,a12=12,a13=13,a14=14,a15=15,a16=16,a17=17,a18=18,a19=19,a20=20;
          function inner() {
            let s = 0;
            for (let i = 0; i < 100000; i++) s += a1 + a20;
            return s;
          }
          return inner();
        }
        outer();
    ";
    let missed = misses(source);
    assert!(
        missed < 100,
        "a captured name missed {missed} times over {ACCESSES} accesses \
         ({:.4}% hit) — an environment past the inline slots is resolving by name",
        hit_rate(missed)
    );
}

/// A method found on a prototype is cached at the site that calls it rather than
/// re-walked per call, and inheritance does not change that.
#[test]
fn a_method_reached_through_a_prototype_chain_is_cached_at_its_site() {
    let source = "
        class B { constructor() { this.v = 1; } read() { return this.v; } }
        class C extends B {}
        const c = new C();
        let s = 0;
        for (let i = 0; i < 100000; i++) s += c.read();
        s;
    ";
    let missed = misses(source);
    assert!(
        missed < 200,
        "a prototype method missed {missed} times over {ACCESSES} accesses \
         ({:.4}% hit) — the chain is being walked per call",
        hit_rate(missed)
    );
}

/// A site that alternates between two layouts learns BOTH and then stops asking.
///
/// # What this replaced
///
/// Until 2026-08-25 a cell remembered one layout — the header at word zero, the
/// offset at word one — so two layouts at one site overwrote each other's entry
/// on every access and the site never converged. This test existed then too,
/// asserting the defect: **100 009 misses over 100 000 accesses**, with
/// `RTS_CACHE_CENSUS=1` reporting "100009 resolver entries, 0 refused" — every
/// one SUCCEEDED. The cache was not failing to answer, it was being asked every
/// time, which is a monomorphic inline cache meeting polymorphic code.
///
/// A second entry in words three through five answers it: `cache::remember`
/// demotes the displaced entry instead of dropping it, and `lower_cached_get`
/// compares against both. Measured the same day, same release build: **11**.
/// An alternating site now misses twice and hits from then on.
#[test]
fn a_site_that_alternates_between_two_layouts_learns_both() {
    let source = "
        function one() { return { a: 1, b: 2 }; }
        function two() { return { b: 2, a: 1 }; }
        const xs = [one(), two()];
        let s = 0;
        for (let i = 0; i < 100000; i++) s += xs[i & 1].a;
        s;
    ";
    let missed = misses(source);
    assert!(
        missed < 100,
        "a two-layout site missed {missed} times over {ACCESSES} accesses \
         ({:.4}% hit) — the second entry is not being consulted or not being \
         filled",
        hit_rate(missed)
    );
}

/// Three layouts at one site still thrash, and this pins the LIMIT rather than
/// the defect.
///
/// # Why two entries and not four
///
/// A read entry is three words — layout, offset, base — and the cell is eight,
/// sized to sixty-four bytes because that is one cache line. Two entries fit in
/// the line already paid for; four would need a second line on every access,
/// including at the monomorphic sites that are the overwhelming majority. That
/// is a cost every site pays to help the few, and it was declined.
///
/// So a third layout evicts, and the site is back to asking. Measured
/// 2026-08-25: three layouts miss once per access, the same as two did before
/// the second entry existed. It is asserted rather than left implied, because
/// the number that says how far this goes is worth as much as the one that says
/// it works — and if a wider cell ever lands, this is the test that says what it
/// bought.
#[test]
fn a_site_that_sees_three_layouts_evicts_and_asks_again() {
    let source = "
        function one() { return { a: 1, b: 2 }; }
        function two() { return { b: 2, a: 1 }; }
        function three() { return { c: 3, a: 1 }; }
        const xs = [one(), two(), three()];
        let s = 0;
        for (let i = 0; i < 100000; i++) s += xs[i % 3].a;
        s;
    ";
    let missed = misses(source);
    assert!(
        missed >= ACCESSES / 2,
        "a three-layout site missed only {missed} times over {ACCESSES} \
         accesses ({:.2}% hit) — if this is now low, the cell holds more than \
         two entries and this test should say what that bought",
        hit_rate(missed)
    );
}
