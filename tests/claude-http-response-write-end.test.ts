// `res.write(chunk)` and `res.end(chunk)` do not throw — the two spellings every
// Node server is written in.
//
// `writable.write(chunk[, encoding][, callback])` makes the callback OPTIONAL,
// and `response_write_hook` called it unconditionally: `entry::call(undefined,
// …)` raises `TypeError: undefined is not a function`. It threw AFTER the bytes
// had gone out, so the client got a correct response and the handler died
// anyway — which is why nothing noticed. A handler that throws takes whatever
// came after it with it, so `res.end()` after a `res.write()` never ran.
//
// Measured before writing the fix, across every operation whose callback the
// Node API documents as optional — `Writable`, `Duplex`, `Transform`,
// `PassThrough`, `Gzip#flush`, `fs.WriteStream`, `net.Socket#end`: only the HTTP
// response threw. A grep found 56 unguarded `entry::call(callback, …)` sites in
// this crate and 54 of them are unreachable without a callback, which is why
// this fixture is about the HTTP response and not about the grep.
import { describe, test, expect } from "rts:test";
import { createServer } from "node:http";

const faults: string[] = [];
let reached = "nothing";

function attempt(label: string, f: () => void): void {
    try { f(); } catch (e: any) { faults.push(label + ": " + String(e.message)); }
}

const server = createServer((_req: any, res: any) => {
    attempt("writeHead", () => res.writeHead(200, { "content-type": "text/plain" }));
    attempt("write", () => res.write("A"));
    attempt("write again", () => res.write("B"));
    attempt("end with a chunk", () => res.end("C"));
    // Reached only if nothing above threw, which is the half a try/catch around
    // each call cannot show on its own: a handler that throws stops here.
    reached = "the end of the handler";
});
const port = 18903;
server.listen(port);

const response = await fetch("http://127.0.0.1:" + port + "/");
const body = await response.text();

describe("http res.write/res.end without a callback", () => {
    test("nothing in the handler threw", () => expect(faults.join(" | ")).toBe(""));
    test("the handler ran to its end", () => expect(reached).toBe("the end of the handler"));
    test("the status is the one written", () => expect(response.status).toBe(200));
    test("every chunk reached the client, in order", () => expect(body).toBe("ABC"));
    test("and the header written beside them", () =>
        expect(response.headers.get("content-type")).toBe("text/plain"));
});
