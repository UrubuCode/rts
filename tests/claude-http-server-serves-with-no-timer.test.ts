// An `http.createServer(...).listen(...)` serves a real request with NO timer
// holding the process up — which is the whole of issue #2893.
//
// `claude-http-loopback-exchange` already pins that the exchange completes, and
// it could not pin this: its watchdog is a ref'd `setTimeout(…, 6000)`, so the
// program it measures is held open by the watchdog rather than by the server.
// Remove the watchdog from that fixture and it exits before the request lands.
// Measured on `1d070295c`:
//
//   $ rts run p2_http.ts   -> "listening", rc=0, immediately; curl refused
//   $ node                 -> alive indefinitely
//
// # Why the dial is DEFERRED, which is the whole discriminator
//
// The first draft of this file dialled from inside the `listen` callback and
// passed on `1d070295c` — the broken engine — because an open socket has
// answered `Pending::In` since long before this change. The client's own socket
// held the loop open, so the exchange completed and the file measured nothing
// it claimed to. That is the green vacuum this repository keeps finding: an
// assertion that cannot fail for the reason it names.
//
// So the request is issued from an UNREF'd timer instead. An unrefed timer is
// pumped on every pass and holds nothing open, so it can only fire if something
// else keeps the loop turning — and between `listen` and the dial the listener
// is the only candidate. On the broken engine nothing is outstanding at that
// point, the parked `await` below raises "this promise cannot settle", and the
// file is red in milliseconds. Measured both ways before being committed.
import { describe, test, expect } from "rts:test";
import { createServer, request } from "node:http";

const seen: string[] = [];
let finish: (value: string) => void = () => {};
const served: Promise<string> = new Promise((resolve: any) => {
  finish = resolve;
});

const watchdog: any = setTimeout(() => {
  throw new Error("no exchange in 6s with nothing but the server open; saw [" + seen.join(",") + "]");
}, 6000);
watchdog.unref();

const server = createServer((_req: any, res: any) => {
  seen.push("handled");
  res.writeHead(200, { "Content-Type": "text/plain" });
  res.end("ok");
});

function dial(port: number): void {
  seen.push("dialling");
  const outgoing = request({ host: "127.0.0.1", port, path: "/", method: "GET" }, (res: any) => {
    seen.push("status " + res.statusCode);
    let body = "";
    res.on("data", (chunk: any) => {
      body += chunk.toString();
    });
    res.on("end", () => finish(body));
  });
  outgoing.on("error", (error: any) => {
    throw new Error("request errored: " + error.code + "; saw [" + seen.join(",") + "]");
  });
  outgoing.end();
}

server.listen(0, "127.0.0.1", () => {
  seen.push("listening");
  const port = server.address().port;
  // Unrefed, and that is the discriminator — see the header. A ref'd timer here
  // would hold the program open by itself and this file would pass on the
  // engine it exists to refuse.
  const later: any = setTimeout(() => dial(port), 60);
  later.unref();
});

const body = await served;
clearTimeout(watchdog);
server.close();

describe("an http server serves without a timer holding the process up", () => {
  test("the handler ran and the client read the body", () => {
    expect(body).toBe("ok");
    expect(seen.join(",")).toBe("listening,dialling,handled,status 200");
  });
});
