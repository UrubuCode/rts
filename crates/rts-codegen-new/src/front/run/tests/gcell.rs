//! Module-level mutable globals (epic #195) — a top-level `let` WRITTEN from
//! inside a function is promoted to a runtime CELL (`funcval::module_globals` +
//! `gcell.rs` + the runtime `__RTS_FN_NS_GC_GCELL_*`). Every access (top-level +
//! the function) goes through the cell, so the mutation is genuinely shared — the
//! by-value capture snapshot no longer applies. This is the gate that unblocks the
//! rts:test harness `print` helper (a void fn writing a captured top-level `let`).
//!
//! Each test runs a REAL `.ts` program end to end and asserts EXACT captured
//! stdout — the honesty floor.

use super::assert_stdout;

#[test]
fn counter_compound_assign() {
    assert_stdout(
        "let c = 0; function inc() { c += 1; } inc(); inc(); inc(); console.log(c);",
        "3\n",
    );
}

#[test]
fn string_accumulator() {
    // The exact rts:test harness shape: a void fn writing a captured top-level let.
    assert_stdout(
        r#"let s = ""; function add(v: string): void { s += v; } add("a"); add("b"); console.log(s);"#,
        "ab\n",
    );
}

#[test]
fn inc_operator_on_global() {
    assert_stdout(
        "let n = 5; function bump() { n++; } bump(); bump(); console.log(n);",
        "7\n",
    );
}

#[test]
fn plain_assign_from_function() {
    assert_stdout(
        "let g = 1; function setit() { g = 42; } setit(); console.log(g);",
        "42\n",
    );
}

#[test]
fn read_reflects_function_write() {
    // The top-level read after the call sees the function's mutation (shared cell,
    // not a stale snapshot).
    assert_stdout(
        "let g = 5; function f() { g = g + 1; return g; } console.log(f()); console.log(g);",
        "6\n6\n",
    );
}

#[test]
fn param_shadows_global_cell() {
    // A function PARAM of the same name shadows the global cell — the write is to
    // the local param, the global stays unchanged.
    assert_stdout(
        "let x = 10; function f(x: number): number { x = x + 1; return x; } console.log(f(1)); console.log(x);",
        "2\n10\n",
    );
}

#[test]
fn accumulate_in_loop() {
    // Repeated appends through the cell in a loop accumulate correctly (the cell is
    // re-read/re-written each iteration; the growing string handle is a GC root via
    // mark_gcell_roots). `i` is a top-level loop var written ONLY at top level, so
    // it stays a normal local (not a cell) — only `out` (written in `p`) is a cell.
    assert_stdout(
        r#"let out = ""; function p(v: string): void { out += v; }
           let i = 0; while (i < 3) { p("ab"); i = i + 1; }
           console.log(out);"#,
        "ababab\n",
    );
}
