// `fetch` must send the headers the program passed, in every init shape.
// Pins the defect of #2833: `Headers.prototype.entries()` answers an ITERATOR
// since #2768, and the native read it by `length` — so zero pairs went out and
// a JSON-RPC server answered 415 to every POST. Asserted against a local
// `node:http` server, which is what sees the request line for real; a remote
// status code would also depend on the network.
//
// The handler calls `res.end("ok")` plainly. It used to need a `try` around it,
// because `res.end(chunk)` threw "undefined is not a function" — a separate
// `node:http` defect this file had to work around and say so. That is fixed
// (#2841), so the `try` is gone: a `catch` kept past the defect it was written
// for is a comment that lies about the engine.
import { describe, test, expect } from "rts:test";
import { createServer } from "node:http";

const seen: any[] = [];

const server = createServer((req: any, res: any) => {
    seen.push(req.headers);
    res.end("ok");
});
// A fixed port rather than `listen(0)`: `server.address()` is not implemented
// here, so there is no way to ask which port port zero chose.
const port = 18833;
server.listen(port);
const base = "http://127.0.0.1:" + port + "/";

async function send(headers: any): Promise<any> {
    const r = await fetch(base, { method: "POST", headers, body: "{}" });
    await r.text();
    return seen[seen.length - 1];
}

const plain = await send({ "Content-Type": "application/json", "X-Token": "abc" });
const built = await send(new Headers({ "Content-Type": "application/json" }));
const paired = await send([["Content-Type", "application/json"]]);

// Closed because a listening server now keeps the program running (#2893), as
// it does in Node. This file had no `close()` and did not need one only while
// the engine ended a program with a bound listener still in it — so the missing
// call was never correct, it was unobservable.
server.close();

describe("fetch sends request headers (#2833)", () => {
    test("plain object: content-type", () => expect(plain["content-type"]).toBe("application/json"));
    test("plain object: second header", () => expect(plain["x-token"]).toBe("abc"));
    test("new Headers(...)", () => expect(built["content-type"]).toBe("application/json"));
    test("array of pairs", () => expect(paired["content-type"]).toBe("application/json"));
    test("entries() is an iterator, not an array", () =>
        expect(Array.isArray(new Headers({ a: "b" }).entries())).toBe(false));
});
