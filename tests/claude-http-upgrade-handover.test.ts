// A `ClientRequest` whose answer is a `101 Switching Protocols` hands the
// socket to `req.on('upgrade', (res, socket, head) => …)` — and `head` carries
// the bytes that arrived with the headers.
//
// `'upgrade'` did not exist. A `101` was read as an ordinary bodyless response
// (`response_reader::bodyless` answers `true` for it), so `'upgrade'` never
// fired — and that is the ONLY place `ws` from `node_modules` calls
// `setSocket`: `websocket.js` builds its handshake with `https.request` and
// waits for this event and nothing else. So every `new WebSocket("wss://…")`
// through it sat silent forever, which is where `@whiskeysockets/baileys`
// stopped on 2026-10-03 — one `connection.update` saying `connecting`, then 45
// seconds of nothing: no QR, no second update, no error.
//
// Node 22 on this same program, measured 2026-10-03:
//   listening,server:raw,client:upgrade 101 upgrade=websocket head=HELLO,
//   client:echo=PONG
//
// Plain `http` and loopback on purpose. What is under test is the CLIENT's
// reading of a `101`, which `http` and `https` share whole — `https::client`
// builds its `ClientRequest` on `http.ClientRequest.prototype` and
// `response_reader` is what both read through, so a `101` over TLS takes this
// same path. A TLS version of this ruler is NOT possible today for a reason
// that is nothing to do with `101`: `tls.createServer` and `tls.connect` in one
// program never complete a handshake with each other at all (measured
// 2026-10-03: `listening` and then silence, no `'secureConnection'`), which is
// its own defect and is recorded as one.
//
// The server is a RAW `net.Server`, not `http.createServer`: an upgrade is a
// response our own server module cannot write (it frames every answer as an
// `http.ServerResponse`), and writing the bytes by hand is also what lets the
// new protocol's first bytes ride in the SAME write as the headers — that is
// what `head` is for, and a server that wrote them separately would never
// exercise it.
//
// The watchdog is what makes absence loud: the failure here is silence
// followed by a clean exit.
import { describe, test, expect } from "rts:test";
import { createServer } from "node:net";
import { request } from "node:http";

const seen: string[] = [];
const failures: string[] = [];

const watchdog = setTimeout(() => {
  throw new Error("no upgrade handover in 8s; saw [" + seen.join(",") + "]");
}, 8000);

const UPGRADE = "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\r\nHELLO";

const server: any = createServer((socket: any) => {
  let pending = "";
  socket.on("data", (chunk: any) => {
    pending += chunk.toString();
    if (pending.indexOf("\r\n\r\n") < 0) return;
    if (pending.indexOf("/ws") >= 0) {
      seen.push("server:raw");
      pending = "";
      socket.write(UPGRADE);
      // Whatever arrives down the handed-over socket comes back uppercased,
      // which is the proof the socket is USABLE after the handover and not
      // merely passed.
      socket.on("data", (more: any) => socket.write(more.toString().toUpperCase()));
      return;
    }
    pending = "";
  });
});

function expectText(got: string, want: string, what: string): void {
  if (got !== want) {
    failures.push(what + " was [" + got + "], wanted [" + want + "]");
  }
}

function finish(): void {
  clearTimeout(watchdog);
  server.close();
  if (failures.length > 0) {
    throw new Error(failures.join(" | ") + "; saw [" + seen.join(",") + "]");
  }
  console.log("claude-http-upgrade-handover: " + seen.join(","));
}

server.listen(0, "127.0.0.1", () => {
  seen.push("listening");
  const port = server.address().port;
  const req: any = request({
    host: "127.0.0.1",
    port,
    path: "/ws",
    method: "GET",
    headers: { Connection: "Upgrade", Upgrade: "websocket" },
  });
  // A `101` reported as `'response'` is the OLD behaviour, and the whole point
  // of the change is that it is not reported that way when an `'upgrade'`
  // listener exists. Node does the same.
  req.on("response", () => failures.push("a 101 was reported as 'response', not 'upgrade'"));
  req.on("error", (error: any) => {
    failures.push("request errored: " + error.code + " " + error.message);
    finish();
  });
  req.on("upgrade", (res: any, socket: any, head: any) => {
    seen.push("client:upgrade " + res.statusCode + " upgrade=" + res.headers.upgrade + " head=" + head.toString());
    if (res.statusCode !== 101) failures.push("statusCode was " + res.statusCode + ", wanted 101");
    expectText(res.headers.upgrade, "websocket", "res.headers.upgrade");
    // The bytes that arrived WITH the headers, which no response body would
    // ever have carried.
    expectText(head.toString(), "HELLO", "head");
    socket.on("data", (chunk: any) => {
      seen.push("client:echo=" + chunk.toString());
      expectText(chunk.toString(), "PONG", "echo over the handed-over socket");
      // Destroyed because `server.close()` waits for open connections and an
      // upgraded socket never closes itself: Node hangs on this same program
      // without this line — checked, not assumed.
      socket.destroy();
      finish();
    });
    socket.write("pong");
  });
  req.end();
});

describe("a 101 is handed to 'upgrade', not read as a response", () => {
  test("the server reports the port the client has to dial", () => {
    expect(typeof server.address).toBe("function");
  });
});
