// node:console — the `Console` CLASS (`new Console(stdout, stderr?)`),
// distinct from the global `console` (rts-std, already covered elsewhere).
//
// `crates/rts-node/src/console.rs`'s own `//!` doc claims TWO things this
// fixture checks by execution rather than by reading:
//   1. the constructor doc-comment says it accepts BOTH
//      `new Console(stdout, stderr?)` AND `new Console({ stdout, stderr? })`
//      — but `build()` never destructures an options object, it only ever
//      treats its 2nd/3rd positional args as the streams themselves.
//   2. `table()`'s doc-comment says it "falls back to log for anything that
//      is not array-like (structural check)" — but the code has no such
//      check at all; it is unconditionally `log`, always.
// Every stream here is a plain `{ write(s) {...} }` object — the module's
// own doc says that is all a bound stream needs to be for THIS engine (no
// `removeListener` etc. required, unlike real Node's Console which needs a
// real Writable — verified against real Node with a real `stream.Writable`,
// see the comments below).

import { describe, test, expect } from "rts:test";
import { Console } from "node:console";

function capture() {
    const chunks: string[] = [];
    return { chunks, stream: { write(s: any) { chunks.push(String(s)); return true; } } };
}

// --- positional constructor: new Console(stdout, stderr) -------------------
const out = capture();
const err = capture();
const c = new Console(out.stream, err.stream);

c.log("hello", 1, true);
const logOk = out.chunks[0] === "hello 1 true\n";

c.error("bad", 2);
const errorOk = err.chunks[0] === "bad 2\n";
c.warn("careful");
const warnOk = err.chunks[1] === "careful\n";

// --- group / groupEnd indentation -------------------------------------------
out.chunks.length = 0;
c.group("G");
c.log("inside");
c.groupEnd();
c.log("outside");
// Verified against real Node (stream.Writable capture): group() writes the
// label at the CURRENT indent, then every following line up to groupEnd() is
// indented by two spaces.
const groupOk = out.chunks[0] === "G\n" && out.chunks[1] === "  inside\n" && out.chunks[2] === "outside\n";

// groupEnd() below zero is a no-op, never negative indent.
out.chunks.length = 0;
c.groupEnd();
c.groupEnd();
c.log("floored");
const groupFlooredOk = out.chunks[0] === "floored\n";

// --- count / countReset ------------------------------------------------------
// Isolated finding: `count()` called with NO argument reads "undefined: N"
// here, not "default: N" — confirmed against real Node directly (v20:
// `console.count()` twice prints "default: 1" / "default: 2"). Read in
// `console.rs`: `count`'s label param falls back to `"default"` only when
// `entry::text_of(label)` answers `None`; a bare `count()` apparently hands
// it a real JS `undefined` VALUE that stringifies to `"undefined"` rather
// than the ABI's "argument not supplied" placeholder, so the fallback never
// fires. Asserting Node's real answer, expected to stay RED.
out.chunks.length = 0;
c.count();
c.count();
const defaultLabelOk = out.chunks[0] === "default: 1\n" && out.chunks[1] === "default: 2\n";

// countReset zeroes the label rather than deleting it, so the NEXT count()
// for that label reads 1 again, same as a label never counted before.
// Verified against real Node directly. Uses an explicit label throughout so
// the "default" bug above cannot muddy this assertion.
out.chunks.length = 0;
c.count("x");
c.countReset("x");
c.count("x");
const countResetOk = out.chunks[0] === "x: 1\n" && out.chunks[1] === "x: 1\n";

// --- time / timeEnd -----------------------------------------------------------
out.chunks.length = 0;
c.time("t");
c.timeEnd("t");
const timeEndOk = /^t: \d+(\.\d+)?ms\n$/.test(out.chunks[0]);
// timeEnd() for a label never started is a silent no-op (no throw, no line).
out.chunks.length = 0;
c.timeEnd("never-started");
const timeEndMissingOk = out.chunks.length === 0;

// --- assert --------------------------------------------------------------------
err.chunks.length = 0;
c.assert(false, "boom", 1);
const assertFalseOk = err.chunks[0] === "Assertion failed: boom 1\n";
err.chunks.length = 0;
c.assert(true, "not printed");
const assertTrueOk = err.chunks.length === 0;

// --- trace -----------------------------------------------------------------
// Real Node prefixes "Trace: " and appends the real call stack
// ("\n    at ..." lines). This crate's own doc says the stack itself is not
// fabricated — only the prefix and the formatted arguments are written.
// Asserting Node's real shape here on purpose, per this task's instructions:
// a stack line is expected and is NOT what this engine will produce.
err.chunks.length = 0;
c.trace("hi");
const tracePrefixOk = err.chunks[0].indexOf("Trace: hi") === 0;
const traceHasStackOk = err.chunks[0].indexOf("\n    at ") >= 0;

// --- dir -----------------------------------------------------------------------
out.chunks.length = 0;
c.dir({ a: 1, b: [1, 2] });
// Loose check (exact util.inspect spacing is node_util's own concern, not
// this module's) — real Node: "{ a: 1, b: [ 1, 2 ] }\n".
const dirOk = out.chunks[0].indexOf("a: 1") >= 0 && out.chunks[0].indexOf("1, 2") >= 0;

// --- format specifiers (%s / %d) inside log ---------------------------------
// Real Node's console.log runs util.format(-like) substitution when the
// first argument is a string containing specifiers: "%s-%d" with ("a", 3)
// prints "a-3". Verified against real Node (v20) directly. This module's
// own doc says specifiers are NOT implemented in log/error — every argument
// is independently inspected and space-joined instead, so this engine's
// answer is expected to be "%s-%d a 3", not "a-3". Asserting Node's real
// answer here, which is expected to stay RED.
out.chunks.length = 0;
c.log("%s-%d", "a", 3);
const formatSpecifierOk = out.chunks[0] === "a-3\n";

// --- table() ---------------------------------------------------------------
// Real Node draws a box-drawing table with an "(index)" header column for
// array-like input. Verified against real Node (v20) directly (see the
// fixture's own header comment for the literal output). This module's
// table() is, per direct code reading, unconditionally identical to log() —
// no array-like branch exists despite the doc-comment's claim of one.
// Asserting Node's real shape, expected to stay RED.
out.chunks.length = 0;
c.table([{ a: 1, b: 2 }, { a: 3, b: 4 }]);
const tableHasIndexColumnOk = out.chunks[0].indexOf("(index)") >= 0;

// --- constructor: new Console({ stdout, stderr }) options-object form ------
// Verified against real Node (v20) directly: this form works there.
const opt = capture();
const c2 = new Console({ stdout: opt.stream, stderr: opt.stream } as any);
c2.log("via options object");
const optionsFormOk = opt.chunks[0] === "via options object\n";

describe("node:console Console (positional constructor)", () => {
    test("log formats and space-joins onto __stdout", () => expect(logOk).toBe(true));
    test("error/warn go to __stderr", () => {
        expect(errorOk).toBe(true);
        expect(warnOk).toBe(true);
    });
    test("group()/groupEnd() indent by two spaces", () => expect(groupOk).toBe(true));
    test("groupEnd() past zero floors at zero", () => expect(groupFlooredOk).toBe(true));
    test("countReset() zeroes rather than deletes the counter", () => expect(countResetOk).toBe(true));
    test("time()/timeEnd() writes an elapsed-ms line", () => expect(timeEndOk).toBe(true));
    test("timeEnd() on an unstarted label is a silent no-op", () => expect(timeEndMissingOk).toBe(true));
    test("assert(false, ...) writes 'Assertion failed: ...'", () => expect(assertFalseOk).toBe(true));
    test("assert(true, ...) writes nothing", () => expect(assertTrueOk).toBe(true));
    test("trace() prefixes 'Trace: '", () => expect(tracePrefixOk).toBe(true));
    test("trace() includes a real call stack (Node does; this engine cannot)", () =>
        expect(traceHasStackOk).toBe(true));
    test("dir() structurally inspects an object", () => expect(dirOk).toBe(true));
});

describe("node:console Console — divergences from real Node", () => {
    test("count() with no argument uses label 'default', per real Node", () =>
        expect(defaultLabelOk).toBe(true));
    test("log('%s-%d', 'a', 3) substitutes format specifiers, per real Node", () =>
        expect(formatSpecifierOk).toBe(true));
    test("table() draws a real table with an (index) column, per real Node", () =>
        expect(tableHasIndexColumnOk).toBe(true));
});

describe("node:console Console (options-object constructor)", () => {
    test("new Console({ stdout, stderr }) binds the streams, per real Node", () =>
        expect(optionsFormOk).toBe(true));
});
