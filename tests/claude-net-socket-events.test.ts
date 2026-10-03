// A program whose only pending work is a SOCKET still reaches its events.
//
// `node:net` registers itself as a loop source, so the mechanism was present —
// but the source answered `Pending::Blocked`, which by `entry::loops`' own
// contract neither holds the program open nor bounds the host's sleep. So a
// program that opened a socket and only waited exited before the connect thread
// had queued anything, and a program kept alive by an unrelated `setTimeout`
// slept straight to that timer's deadline and ran the timer — which called
// `process.exit` — before `net` was ever pumped. Measured against Node 22: the
// whole sequence below arrives in milliseconds there and NOTHING arrived here.
//
// Everything runs over `127.0.0.1` with the server in this same program: the
// behaviour pinned is event DELIVERY, not reachability of any host, and a ruler
// that needs the internet measures the internet.
//
// The watchdog is what makes absence loud. The failure this pins is silence —
// no event, clean exit, exit code 0 — which a fixture that only asserts on
// arrival would report as a pass.
import { describe, test, expect } from "rts:test";
import { createServer, connect } from "node:net";

const seen: string[] = [];
let verdict = "watchdog never ran";

const watchdog = setTimeout(() => {
  throw new Error("no socket events in 4s; saw [" + seen.join(",") + "]");
}, 4000);

function finish(): void {
  clearTimeout(watchdog);
  server.close();
  const order = seen.join(",");
  const want = "listening,connection,connect,data:pong,end,close";
  if (order !== want) {
    throw new Error("event sequence was [" + order + "], wanted [" + want + "]");
  }
  verdict = "ok";
  console.log("claude-net-socket-events: " + order);
}

const server = createServer((incoming: any) => {
  seen.push("connection");
  incoming.end("pong");
});

server.listen(0, "127.0.0.1", () => {
  seen.push("listening");
  const port = server.address().port;
  const socket = connect({ host: "127.0.0.1", port });
  socket.on("connect", () => seen.push("connect"));
  socket.on("data", (chunk: any) => seen.push("data:" + chunk.toString()));
  socket.on("end", () => seen.push("end"));
  socket.on("close", () => {
    seen.push("close");
    finish();
  });
  socket.on("error", (error: any) => {
    clearTimeout(watchdog);
    server.close();
    throw new Error("socket errored: " + error.code);
  });
});

describe("node:net is a loop source a waiting program is served by", () => {
  test("the module exposes the two halves this fixture waits on", () => {
    expect(typeof connect).toBe("function");
    expect(typeof createServer).toBe("function");
  });
});
