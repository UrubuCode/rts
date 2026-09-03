// node:readline — createInterface over a node:stream Readable/Writable pair
// (never over process.stdin, which nothing in this suite writes to), plus
// the cursor-control ANSI writers. Every assertion here was checked against
// `node -e "..."` (Node v20.19.5) at the same time this file was written.
//
// `close`/`line` ordering note: readline's own `close()` (called directly,
// or via the 'end' listener build_interface wires onto `input`) is correct
// and idempotent (confirmed below). What is NOT reachable here is 'close'
// firing from `input` naturally ending (`input.push(null)`) — that needs
// node:stream's Readable to emit 'end', and it does not (confirmed: a bare
// `Readable` with a 'data' listener attached never emits 'end' after
// `push(null)`, with or without readline in the picture — see
// tests/claude-node-events-on.test.ts's sibling report for where that line
// is drawn; this is a node:stream defect, not this crate's readline code).
// So the natural-EOF-closes-the-interface test below is expected RED, and
// the direct `rl.close()` idempotency test right above it is expected GREEN
// — same Interface, two different trigger paths.
import { describe, test, expect } from "rts:test";
import { createInterface, clearLine, clearScreenDown, cursorTo, moveCursor } from "node:readline";
import { Readable, Writable } from "node:stream";

function capturingWritable(sink: string[]): any {
    return new Writable({ write: (chunk: any, _enc: any, cb: any) => { sink.push(String(chunk)); cb(); } });
}

// --- line splitting over 'data' ---------------------------------------------
const lineOut: string[] = [];
const lineInput: any = new Readable({ read: () => {} });
const lineOutput: any = capturingWritable([]);
const lineRl: any = createInterface({ input: lineInput, output: lineOutput });
lineRl.on("line", (line: string) => { lineOut.push(line); });
lineInput.push("hello\nworld\n");
lineInput.push("partial"); // no trailing \n yet: must NOT appear as a line
const lineOutBeforeFlush = lineOut.slice();
lineInput.push(" line\n");

// --- rl.close() direct, and idempotent --------------------------------------
const closeInput: any = new Readable({ read: () => {} });
const closeOutput: any = capturingWritable([]);
const closeRl: any = createInterface({ input: closeInput, output: closeOutput });
let closeCount = 0;
closeRl.on("close", () => { closeCount++; });
closeRl.close();
closeRl.close(); // second call must not fire 'close' again

// --- natural EOF (push(null)) closing the interface -------------------------
// Expected RED: see the module doc above — this needs node:stream's
// Readable to emit 'end', which it does not.
const eofInput: any = new Readable({ read: () => {} });
const eofOutput: any = capturingWritable([]);
const eofRl: any = createInterface({ input: eofInput, output: eofOutput });
let eofClosed = false;
eofRl.on("close", () => { eofClosed = true; });
eofInput.push("bye\n");
eofInput.push(null);

// --- setPrompt / getPrompt / prompt() / write() -----------------------------
const promptWritten: string[] = [];
const promptOutput: any = capturingWritable(promptWritten);
const promptInput: any = new Readable({ read: () => {} });
const promptRl: any = createInterface({ input: promptInput, output: promptOutput });
promptRl.setPrompt("$ ");
const promptGot = promptRl.getPrompt();
promptRl.prompt();
promptRl.write("typed");
const promptWrittenSnapshot = promptWritten.slice();

// --- rl.question(query, callback) — callback form ---------------------------
const qWritten: string[] = [];
const qOutput: any = capturingWritable(qWritten);
const qInput: any = new Readable({ read: () => {} });
const qRl: any = createInterface({ input: qInput, output: qOutput });
let qAnswer = "";
qRl.question("What is your name? ", (a: string) => { qAnswer = a; });
const qWrittenAfterAsk = qWritten.slice();
qInput.push("Ada\n");

// --- clearLine / clearScreenDown / cursorTo: byte-exact ANSI ---------------
// Confirmed byte-for-byte against `node -e "readline.clearLine(...)"` etc.
function capture(fn: (s: any) => void): string {
    const chunks: string[] = [];
    fn(capturingWritable(chunks));
    return chunks.join("");
}
const clearLineLeft = capture((s) => clearLine(s, -1));
const clearLineRight = capture((s) => clearLine(s, 1));
const clearLineBoth = capture((s) => clearLine(s, 0));
const clearScreen = capture((s) => clearScreenDown(s));
const cursorXOnly = capture((s) => cursorTo(s, 5));
const cursorXY = capture((s) => cursorTo(s, 5, 3));

// --- moveCursor: BUG — RTS writes the dy sequence before dx; Node writes ---
// dx before dy. Confirmed live both ways:
//   node -e "readline.moveCursor(s, 3, -2)"   -> "\x1b[3C\x1b[2A"
//   RTS    moveCursor(s, 3, -2)                -> "\x1b[2A\x1b[3C"
// This assertion states Node's order and is expected RED on RTS.
const moveCursorOrder = capture((s) => moveCursor(s, 3, -2));
const moveCursorOrderExpected = "\x1b[3C\x1b[2A";

// --- moveCursor(0,0): BUG — RTS still calls .write("") once; Node calls ---
// .write() zero times. Confirmed live both ways.
const moveCursorZeroChunks = (() => {
    const chunks: string[] = [];
    moveCursor(capturingWritable(chunks), 0, 0);
    return chunks.length;
})();

describe("node:readline createInterface — 'line' splitting", () => {
    test("complete lines fire 'line', trailing partial does not", () => {
        expect(lineOutBeforeFlush.join("|")).toBe("hello|world");
    });
    test("the partial line completes once its newline arrives", () => {
        expect(lineOut.join("|")).toBe("hello|world|partial line");
    });
});

describe("node:readline Interface — close()", () => {
    test("close() emits 'close' exactly once across two calls", () => expect(closeCount).toBe(1));
    // Expected RED — see the module doc above.
    test("input ending naturally (push(null)) also closes the interface (Node)", () => expect(eofClosed).toBe(true));
});

describe("node:readline Interface — prompt/write", () => {
    test("getPrompt reflects setPrompt", () => expect(promptGot).toBe("$ "));
    test("prompt() writes the prompt string", () => expect(promptWrittenSnapshot[0]).toBe("$ "));
    test("write(data) writes the string form", () => expect(promptWrittenSnapshot[1]).toBe("typed"));
});

describe("node:readline Interface — question() callback form", () => {
    test("question() writes the query first", () => expect(qWrittenAfterAsk[0]).toBe("What is your name? "));
    test("the next 'line' answers the callback", () => expect(qAnswer).toBe("Ada"));
});

describe("node:readline — clearLine/clearScreenDown/cursorTo ANSI bytes", () => {
    test("clearLine(-1) clears left of cursor", () => expect(clearLineLeft).toBe("\x1b[1K"));
    test("clearLine(1) clears right of cursor", () => expect(clearLineRight).toBe("\x1b[0K"));
    test("clearLine(0) clears the whole line", () => expect(clearLineBoth).toBe("\x1b[2K"));
    test("clearScreenDown", () => expect(clearScreen).toBe("\x1b[0J"));
    test("cursorTo(x) — column only", () => expect(cursorXOnly).toBe("\x1b[6G"));
    test("cursorTo(x, y) — row and column", () => expect(cursorXY).toBe("\x1b[4;6H"));
});

describe("node:readline — moveCursor (BUGS, asserting Node's answer)", () => {
    // Expected RED — see the comment above moveCursorOrderExpected.
    test("writes the dx escape before the dy escape (Node order)", () => expect(moveCursorOrder).toBe(moveCursorOrderExpected));
    // Expected RED — see the comment above moveCursorZeroChunks.
    test("moveCursor(0,0) writes nothing at all (Node)", () => expect(moveCursorZeroChunks).toBe(0));
});
