//! P5.7: CLOSURES — arrows/functions that CAPTURE outer locals BY VALUE.
//!
//! A capturing arrow used as a value is lifted to a CLOSURE: its captured free
//! vars are snapshotted (by value) into an env array at closure-creation, and the
//! closure thunk reads them back from the env. This unlocks capturing callbacks
//! (`arr.map(x => x * factor)`) and returned closures (`adder(5)` → `x => x + 5`).
//!
//! The soundness boundary (BAIL, never wrong): a closure that ASSIGNS a captured
//! var (mutable capture), a captured var reassigned in the outer scope (a stale
//! snapshot), or a capture of `this` / an unknown name. Capture-by-value is only
//! accepted where the captured value does not change between capture and call.

use super::{assert_bails, assert_stdout};

// ===========================================================================
// Capturing array callbacks (the headline functional gap P4.6/P4.7 bailed on).
// ===========================================================================

#[test]
fn capturing_map() {
    assert_stdout(
        "let f = 10; let a = [1,2,3]; console.log(a.map((x:number) => x * f).join(\",\"));",
        "10,20,30\n",
    );
}

#[test]
fn capturing_filter() {
    assert_stdout(
        "let lo = 2; let a = [1,2,3,4]; console.log(a.filter((x:number) => x > lo).join(\",\"));",
        "3,4\n",
    );
}

#[test]
fn capturing_reduce() {
    assert_stdout(
        "let base = 100; console.log([1,2,3].reduce((acc:number, x:number) => acc + x + base, 0));",
        "306\n",
    );
}

#[test]
fn capturing_map_with_index_arg() {
    // A capture (`off`) AND the callback's own index arg both reach the body: the
    // capture rides the env, the index rides a1. `100 + 1 + 0`, `100 + 2 + 1`.
    assert_stdout(
        "let off = 100; let r = [1,2].map((x:number, i:number) => x + i + off); console.log(r.join(\",\"));",
        "101,103\n",
    );
}

// ===========================================================================
// Returned closures + closures in a variable.
// ===========================================================================

#[test]
fn closure_returned_then_called() {
    // `n` is captured by value at the point `adder(5)` builds the closure.
    assert_stdout(
        "function adder(n: number) { return (x: number) => x + n; } let add5 = adder(5); console.log(add5(10));",
        "15\n",
    );
}

#[test]
fn closure_in_a_variable() {
    assert_stdout(
        "let k = 3; let g = (x: number) => x * k; console.log(g(4));",
        "12\n",
    );
}

#[test]
fn multiple_captures() {
    assert_stdout(
        "let a = 1; let b = 2; let h = (x: number) => x + a + b; console.log(h(10));",
        "13\n",
    );
}

#[test]
fn capture_a_string() {
    // The captured value is a string (a Tagged PolyValue) — the env snapshot boxes
    // it verbatim and the body's `+` concatenates.
    assert_stdout(
        "let prefix = \"hi-\"; let g = (x: number) => prefix + x; console.log(g(5));",
        "hi-5\n",
    );
}

// ===========================================================================
// Two closures over the SAME captured var keep independent by-value snapshots.
// ===========================================================================

#[test]
fn two_closures_same_capture() {
    assert_stdout(
        "let n = 7; let f = (x: number) => x + n; let g = (x: number) => x * n; console.log(f(1), g(2));",
        "8 14\n",
    );
}

// ===========================================================================
// Soundness boundary — BAIL (explicit Unsupported, never a wrong value).
// ===========================================================================

#[test]
fn closure_assigns_captured_top_level_var() {
    // The closure WRITES a captured TOP-LEVEL `let` — now supported (epic #195):
    // `c` is promoted to a module-global CELL, so the write is visible to the outer
    // scope (no by-value snapshot). Previously a documented bail.
    assert_stdout(
        "let c = 0; let inc = () => { c = c + 1; }; inc(); console.log(c);",
        "1\n",
    );
}

#[test]
fn captured_var_reassigned_in_outer_scope_bails() {
    // `factor` is reassigned AFTER the closure is built → the by-value snapshot
    // would be observably stale. Conservatively BAIL on any outer reassignment.
    assert_bails(
        "let factor = 2; let g = (x: number) => x * factor; factor = 3; console.log(g(10));",
    );
}

#[test]
fn capture_of_this_bails() {
    // `this` is not a simple capturable local. BAIL (no env entry for it).
    assert_bails("let g = (x: number) => x + this.k; console.log(g(1));");
}

#[test]
fn top_level_runtime_const_read_in_function() {
    // A top-level `const` initialized to a RUNTIME value (a CALL — not a
    // re-materializable literal) read from inside a plain function resolves to the
    // SAME value via a shared cell — was "unbound identifier `v`". A literal const
    // stays on the by-value path (`capture_a_string`).
    assert_stdout(
        "function mk(): number { return 7; } const v = mk(); \
         function rd(): number { return v + 1; } console.log(rd());",
        "8\n",
    );
}

#[test]
fn async_await_real_spawn_chain() {
    // REAL async (2026-07-02): an async CALL spawns its body on the shared
    // runtime and returns a pending Promise handle; `await` blocks (pumping the
    // event loop) and unwraps the settled value. `: number` (f64) params ride
    // the typed spawn (`__rtsadp_promise_spawn` registers the packed kinds so
    // the invoke reads the bits back into xmm). The top-level `await` keeps the
    // print on the MAIN thread — deterministic capture.
    assert_stdout(
        "async function add(a: number, b: number): Promise<number> { return a + b; } \
         async function run(): Promise<number> { const x = await add(2, 3); return await add(x, 10); } \
         console.log(await run());",
        "15\n",
    );
}

// ===========================================================================
// FUNCTION-LOCAL MUTABLE CAPTURE (#195) — a closure that CAPTURES and MUTATES a
// function-scope `let`. The local is promoted to a per-invocation runtime CELL
// (a 1-slot Vec); the closure captures the cell HANDLE by value, so both sides
// share one mutable box. The headline gap behind the "expression arrow" cluster.
// ===========================================================================

#[test]
fn closure_writes_captured_function_local() {
    // `cb` (an inline arrow in `setup`) WRITES the captured local `count`; the
    // enclosing function reads it back after three calls → the writes are visible
    // (a by-value snapshot could not do this). `count` becomes a cell.
    assert_stdout(
        "function setup(): number { let count = 0; const cb = () => { count = count + 1; }; \
         cb(); cb(); cb(); return count; } console.log(setup());",
        "3\n",
    );
}

#[test]
fn returned_counter_closure_outlives_its_factory() {
    // The closure `() => ++c` is RETURNED from `mk` and called after `mk` returned:
    // the cell (held alive by the closure's captured handle) survives, so the three
    // calls see 1, 2, 3 — independent state per `mk()` invocation.
    assert_stdout(
        "function mk(): () => number { let c = 0; return () => ++c; } \
         const f = mk(); console.log(f(), f(), f());",
        "1 2 3\n",
    );
}

#[test]
fn two_counters_have_independent_cells() {
    // Each `mk()` allocates a FRESH cell, so two counters do not share state.
    assert_stdout(
        "function mk(): () => number { let c = 0; return () => ++c; } \
         const a = mk(); const b = mk(); console.log(a(), a(), b());",
        "1 2 1\n",
    );
}

#[test]
fn captured_mutated_local_compound_assign() {
    // `+=` on a captured-mutated local routes through the cell (read-modify-write).
    assert_stdout(
        "function run(): number { let total = 0; const add = (n: number) => { total += n; }; \
         add(10); add(5); return total; } console.log(run());",
        "15\n",
    );
}

#[test]
fn readonly_capture_stays_by_value_not_a_cell() {
    // A captured local that is NEVER mutated stays the by-value fast path (NOT a
    // cell) — guards against over-promotion. `k` is read-only in the closure.
    assert_stdout(
        "function f(): number { let k = 3; const g = (x: number) => x * k; return g(4); } \
         console.log(f());",
        "12\n",
    );
}

#[test]
fn inline_arrow_callback_reading_console() {
    // An inline arrow CALLBACK that reads `console` (a prelude singleton) — the
    // ubiquitous `xs.forEach(x => console.log(x))` / `ee.on(ev, v => console.log(v))`
    // shape — must lift: `console` is an ambient prelude singleton (a gcell), NOT an
    // unsound capture. Was "expression arrow" because the multi-file path's ambient
    // set omitted prelude singletons.
    assert_stdout(
        "function call(f: (n: number) => void): void { f(7); } call((v: number) => console.log(v));",
        "7\n",
    );
}

#[test]
fn arrow_referencing_class_lifts() {
    // An arrow that references a CLASS NAME (`v instanceof C`, `new C()`) is NOT
    // capturing — the class is a known name, not an outer local. It must lift to a
    // plain top-level fn instead of bailing as an unsound capture.
    assert_stdout(
        "class Foo {} const xs: any[] = [new Foo(), 2]; \
         console.log(xs.filter((v) => v instanceof Foo).length);",
        "1\n",
    );
}

#[test]
fn arrow_referencing_primordial_class_lifts() {
    // Same, for a PRIMORDIAL class (`Error`) referenced in a callback arrow.
    assert_stdout(
        "const xs: any[] = [1, 2]; console.log(xs.filter((v) => v instanceof Error).length);",
        "0\n",
    );
}

#[test]
fn arrow_referencing_ambient_collection_class_lifts() {
    // An AMBIENT `.ts` collection class (`Set`) referenced in a callback arrow —
    // its name comes from the prelude ClassTable (not the user `classes`), so both
    // must seed the arrow extractor's non-capture set.
    assert_stdout(
        "const xs: any[] = [new Set(), 2]; console.log(xs.filter((v) => v instanceof Set).length);",
        "1\n",
    );
}

#[test]
fn function_expression_as_value() {
    // A `function (…) { … }` EXPRESSION lowers to an arrow → the same lift path.
    // Anonymous fn-expr bound to a local and called.
    assert_stdout(
        "const double = function (x: number): number { return x * 2; }; console.log(double(5));",
        "10\n",
    );
}

#[test]
fn iife_function_expression() {
    // An IIFE `(function(a,b){return a+b})(3,4)` — the fn-expr is the callee value.
    assert_stdout(
        "console.log((function (a: number, b: number): number { return a + b; })(3, 4));",
        "7\n",
    );
}

#[test]
fn function_expression_argument() {
    // A fn-expr passed as a callback argument to a HOF.
    assert_stdout(
        "function apply(f: (x: number) => number, x: number): number { return f(x); } \
         console.log(apply(function (x: number): number { return x + 10; }, 5));",
        "15\n",
    );
}

#[test]
fn block_body_arrow_local_used_later() {
    // A BLOCK-body arrow that declares a `const`/`let` and uses it in a LATER
    // statement. The nested free-var scan cloned `bound` per statement, dropping the
    // earlier binding so the local was misreported free → "expression arrow". One
    // shared `bound` across the block fixes it.
    assert_stdout(
        "function call(f: () => void): void { f(); } \
         call(() => { const a = 1; const b = 2; console.log(a + b); });",
        "3\n",
    );
}
