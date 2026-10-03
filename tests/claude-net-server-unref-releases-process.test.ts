// `server.unref()` takes a listener back out of the liveness accounting.
//
// This is the other half of #2893 and the reason it could not be a one-line
// change. Making a listening server hold the program open turns `net.Server`'s
// `ref`/`unref` from a harmless no-op into a false one: both were `noop_self`,
// which was defensible only while a listener held the program open in neither
// direction — there was nothing for them to act on. `CLAUDE.md`'s rule is that
// a surface which cannot do what its name means does not ship, so the methods
// became real in the same change (`ServerEntry::refed`), and
// `net.Socket`'s pair with them: a connected socket has answered
// `Pending::In` for longer still, so `socket.unref()` was ALREADY a method that
// returned `this` and quietly did nothing.
//
// Measured against node 22 on the same program, 2026-10-03:
//
//   server.listen(…); server.unref()   -> node exits immediately; rts exits
//   server.listen(…) alone             -> node stays up; rts stays up
//
// # What this file pins, and what it deliberately does not
//
// It is NOT a discriminator for #2893 — it is red on neither binary, because on
// the old one a listener never held the program open for any reason. It is a
// guard on the new flag: if someone later makes a listener hold the program
// open unconditionally, `unref()` goes quietly back to lying and this file is
// the thing that says so.
//
// Asserted through the deadlock detector rather than through an exit code,
// because that detector reads the SAME accounting the loop does
// (`entry::loops::pump_sources`). So "nothing is outstanding" is observable
// from inside the program, as a catchable throw, and the file can prove it
// without having to fail.
import { describe, test, expect } from "rts:test";
import { createServer } from "node:net";

const server = createServer((connection: any) => connection.end());

let bound = "";
let raised = "";

// Bound synchronously enough to unref? No — `listen` is asynchronous here and
// in Node, so the unref goes in its callback, which is also the only place the
// port is known. Reaching the callback at all needs the pre-bind `In` answer
// `registry::source` gives a server that has not yet bound.
await new Promise<void>((resolve: any) => {
  server.listen(0, "127.0.0.1", () => {
    bound = typeof server.address().port === "number" ? "bound" : "no port";
    server.unref();
    resolve();
  });
});

try {
  // With the only listener unrefed, nothing is outstanding: `Blocked` is pumped
  // but holds nothing open, so this is a genuine deadlock and saying so is
  // correct. Were `unref()` still a no-op, the listener would answer
  // `Pending::In` forever and this would wait here until the harness killed it.
  await new Promise(() => {});
  raised = "nothing was raised";
} catch (error: any) {
  raised = String(error && error.message ? error.message : error);
}

server.close();

describe("server.unref() releases the program", () => {
  test("the listener bound before it was unrefed", () => {
    expect(bound).toBe("bound");
  });

  test("an unrefed listener is not outstanding work", () => {
    expect(raised).not.toBe("nothing was raised");
    expect(raised.indexOf("cannot settle") >= 0).toBe(true);
  });

  test("unref and ref are functions on the server, not absent", () => {
    expect(typeof (server as any).unref).toBe("function");
    expect(typeof (server as any).ref).toBe("function");
  });
});
