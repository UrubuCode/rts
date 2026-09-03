// node:repl — THE killer call, isolated.
//
// Any REPL line whose evaluation throws a RUNTIME exception — an unbound
// name (ReferenceError), a TypeError, an explicit `throw` — crashes the
// WHOLE PROCESS. It is not caught by a `try/catch` wrapped around the
// `input.emit("data", ...)` call that triggers it, because the throw
// happens inside `entry::evaluate(source)`, which compiles and runs the
// line as its own SEPARATE program (crates/rts-node/src/repl.rs's own doc:
// "each line here is a vm.runInNewContext, not an incremental extension of
// one running program") — an uncaught exception in THAT separate program
// reaches this engine's top-level uncaught-exception handler directly,
// which terminates the process, rather than surfacing as a normal
// catchable value back in the caller's program.
//
// This is a straight contradiction of the module's own doc, which claims
// unbound-name failures are "answered as undefined" (the same handling as a
// value that could not cross) — measured directly (see
// claude-node-repl.test.ts's own comment on the string case) that only a
// COMPILE failure gets that treatment. A RUNTIME throw does not.
//
// Practical consequence: a REPL built on this module cannot survive a
// typo. `x` alone, or any expression that dereferences a null, kills the
// process outright — the opposite of what a REPL exists for.
//
// Kept in its own file, with the failing assertion commented out rather than
// executed, exactly as this task's own instructions describe for a killer
// call: this file's job is to name the call precisely, not to run it (a
// crash here would take THIS file's own report with it). The three throwing
// forms below were each independently confirmed, outside `rts:test`, via
// `target/fast/rts.exe run` on a throwaway script:
//
//   input.emit("data", "someUnboundName\n");   // -> process exit 1:
//     rts: uncaught exception (tag 1): ReferenceError: someUnboundName is not defined
//   input.emit("data", "null.foo\n");           // -> process exit 1:
//     rts: uncaught exception (tag 1): TypeError: Cannot read properties of null (reading 'foo')
//   input.emit("data", "throw new Error('boom')\n"); // -> process exit 1:
//     rts: uncaught exception (tag 1): Error: boom
//
// Real Node's REPL survives all three and prints "Uncaught ReferenceError:
// someUnboundName is not defined" (etc.) at the next prompt, per Node's own
// documented behavior — a REPL exists specifically to isolate one line's
// failure from the session. This module's DOES have a try/catch discipline
// elsewhere in this crate for exactly this class of problem (CLAUDE.md rule
// 8 of rts-core/README.md: "a native that calls user code asks whether the
// callee left a throw behind before it looks at the answer") — `repl.rs`'s
// `evaluate_line` does not apply it around `entry::evaluate`.

import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";
import * as repl from "node:repl";

const chunks: string[] = [];
const input = new EventEmitter();
const output = { write: (s: any) => { chunks.push(String(s)); return true; } };
repl.start({ input, output, prompt: "> " });

// The construction above is safe on its own (no throw yet) — asserted so
// this file still measures SOMETHING even with the killer call left unrun.
const constructedOk = chunks[0] === "> ";

// THE KILLER CALL — left commented out on purpose (see header comment).
// Uncommenting ANY of the three lines below takes the whole test process
// down with exit code 1, not a red assertion:
//
// (input as any).emit("data", "someUnboundName\n");
// (input as any).emit("data", "null.foo\n");
// (input as any).emit("data", "throw new Error('boom')\n");

describe("node:repl — construction survives (the crash needs a throwing line)", () => {
    test("repl.start() itself does not crash", () => expect(constructedOk).toBe(true));
});
