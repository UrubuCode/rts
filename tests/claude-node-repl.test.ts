// node:repl — a line-at-a-time REPL over node:readline's Interface
// (crates/rts-node/src/repl.rs). The module's OWN doc states its central
// limit up front: every line runs through `entry::evaluate` as its own
// FRESH, disconnected program — no variable a prior line declared survives
// to the next one, and only a value needing no region (a number, a boolean,
// a singleton) can cross back; anything else — an object, a function, AND
// (per direct measurement below) a STRING — reads as `undefined`, the exact
// same answer a line that failed to compile gets. Both are tested here.
//
// input/output are plain objects: `input` only needs `.on(event, cb)` (an
// `EventEmitter` from node:events supplies that), `output` only needs a
// callable `.write(chunk)` — the same minimal shape `readline.rs` itself
// documents, fed by hand via `input.emit("data", "<line>\n")` rather than a
// real TTY (there is none in this harness).
//
// A SEPARATE, much larger finding is isolated in
// `claude-node-repl-crash.test.ts` instead of here: any line whose
// evaluation THROWS at runtime (an unbound-name ReferenceError, a TypeError,
// an explicit `throw`) crashes the ENTIRE PROCESS, uncatchable even by a
// `try/catch` wrapped around the `input.emit("data", ...)` call site. Only a
// COMPILE failure (a genuine syntax error) is handled gracefully, per the
// module's own doc. Kept out of this file so the rest of this module stays
// measurable.

import { describe, test, expect } from "rts:test";
import { EventEmitter } from "node:events";
import * as repl from "node:repl";

function harness(prompt = "> ") {
    const chunks: string[] = [];
    const input = new EventEmitter();
    const output = { write: (s: any) => { chunks.push(String(s)); return true; } };
    const server: any = repl.start({ input, output, prompt });
    return { chunks, input, server };
}

function feed(h: ReturnType<typeof harness>, line: string) {
    h.chunks.length = 0;
    (h.input as any).emit("data", line + "\n");
    // Node's own shape: the printed value, then a newline, then the next
    // prompt — three writes per line.
    return h.chunks.join("");
}

// --- construction + the initial prompt --------------------------------------
const h1 = harness("myrepl> ");
const serverTypeOk = typeof h1.server === "object";
const initialPromptOk = h1.chunks[0] === "myrepl> ";

// --- a working expression evaluates for real ---------------------------------
const arithOut = feed(h1, "6 * 7");
const arithOk = arithOut === "42\nmyrepl> ";

// --- number/boolean/null cross the fresh-program boundary --------------------
const trueOut = feed(h1, "true");
const trueOk = trueOut === "true\nmyrepl> ";
const falseOut = feed(h1, "1 === 2");
const falseOk = falseOut === "false\nmyrepl> ";
const nullOut = feed(h1, "null");
const nullOk = nullOut === "null\nmyrepl> ";

// --- a STRING result does NOT cross — same answer as a compile failure ------
// Measured directly: `entry::evaluate("'hi'")` answers None (a string needs
// its own region, same as any other heap value per vm.rs's own doc), so this
// prints "undefined" — indistinguishable here from a syntax error. Real
// Node's REPL prints a quoted `'hi'`; this is the module's own documented
// ambiguity, not a surprise this fixture is raising fresh.
const stringOut = feed(h1, "'hi'");
const stringReadsUndefinedOk = stringOut === "undefined\nmyrepl> ";

// --- a genuine SYNTAX error is handled gracefully -----------------------------
// (contrast with claude-node-repl-crash.test.ts's runtime-error case)
const syntaxOut = feed(h1, "(1 +");
const syntaxOk = syntaxOut === "undefined\nmyrepl> ";

// --- THE central limit: no variable survives to the next line ----------------
// `typeof x` never throws even when `x` was never declared (JS's own special
// case for `typeof` on an unbound name), which is what makes this checkable
// WITHOUT hitting the crash in the companion file.
feed(h1, "let x = 5");
const typeofAfterLetOut = feed(h1, "typeof x");
const noPersistenceOk = typeofAfterLetOut === "undefined\nmyrepl> ";

// --- defineCommand / displayPrompt / setupHistory ----------------------------
h1.server.defineCommand("greet", { help: "say hi", action() {} });
const definedOk = typeof h1.server.__commands__ === "object" && typeof h1.server.__commands__.greet === "object";

h1.chunks.length = 0;
h1.server.displayPrompt();
const displayPromptOk = h1.chunks.join("") === "myrepl> ";

let historyErr: any = "unset";
h1.server.setupHistory("/some/path", (err: any) => {
    historyErr = err;
});
// Named rather than silently pretending to succeed: no history file I/O
// exists in this crate, so the callback receives an error every time.
const setupHistoryRefusesOk = typeof historyErr === "string" && historyErr.length > 0;

describe("node:repl — construction and a working expression", () => {
    test("start() returns an object and writes the initial prompt", () => {
        expect(serverTypeOk).toBe(true);
        expect(initialPromptOk).toBe(true);
    });
    test("a working expression is really evaluated: 6 * 7 -> 42", () => expect(arithOk).toBe(true));
});

describe("node:repl — values that cross the fresh-program boundary", () => {
    test("true / (1 === 2) / null all cross and print correctly", () => {
        expect(trueOk).toBe(true);
        expect(falseOk).toBe(true);
        expect(nullOk).toBe(true);
    });
    test("a string result does NOT cross — reads as 'undefined', same as a compile failure", () =>
        expect(stringReadsUndefinedOk).toBe(true));
    test("a genuine syntax error is handled gracefully, not thrown", () => expect(syntaxOk).toBe(true));
});

describe("node:repl — the module's own documented limit: no cross-line state", () => {
    test("`let x = 5` on one line does not make `typeof x` see it on the next", () =>
        expect(noPersistenceOk).toBe(true));
});

describe("node:repl — the rest of the instance surface", () => {
    test("defineCommand() records under __commands__", () => expect(definedOk).toBe(true));
    test("displayPrompt() re-writes the prompt", () => expect(displayPromptOk).toBe(true));
    test("setupHistory() calls back with an error rather than pretending to load one", () =>
        expect(setupHistoryRefusesOk).toBe(true));
});
