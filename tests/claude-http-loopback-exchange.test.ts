// An `http.createServer` and an `http.request` in ONE program over `127.0.0.1`
// complete the exchange.
//
// They could not. `http::client` read the response by spinning on the JavaScript
// thread — `socket.write(empty)` to force `net::registry::pump`, 4 ms of sleep,
// repeat — and `net::registry::pump` carries a reentrancy guard. A same-program
// request starts INSIDE a pump, because `server.listen`'s callback is delivered
// by one, so every nested pump the spin asked for returned without delivering
// anything. The `'connection'` the server needed was in the queue the outer pump
// had already walked past, and the only thing that could deliver it was the loop
// the client was blocking. Measured before: `listening`, then a real
// `ETIMEDOUT` ten seconds later.
//
// Node 22 on the same program, measured 2026-10-02:
//   listening, client:socket, server:request GET /hi, server:req-end,
//   client:response 200, client:data, client:end body=pong
// `server:req-data` does not appear — a GET with no body has none to deliver.
//
// Loopback with both halves here on purpose: what this pins is DELIVERY, and a
// ruler that needs the internet measures the internet.
//
// The watchdog is what makes absence loud, since the failure is silence followed
// by a clean exit. Six seconds and not four: a shorter one is a false red on a
// loaded machine, and this fixture waits on a round trip rather than one event.
import { describe, test, expect } from "rts:test";
import { createServer, request } from "node:http";

const seen: string[] = [];

const watchdog = setTimeout(() => {
  throw new Error("no http exchange and refusal in 6s; saw [" + seen.join(",") + "]");
}, 6000);

const server = createServer((req: any, res: any) => {
  seen.push("server:request " + req.method + " " + req.url);
  req.on("end", () => {
    seen.push("server:req-end");
    res.writeHead(200, { "Content-Type": "text/plain" });
    res.end("pong");
  });
});

let settled = 0;

// Both halves are in flight at once, and neither waits for the other: chaining
// them would mean the second request is made from inside the first's `'end'`
// handler, which keeps a socket open against a server already closed — a shape
// Node itself hangs on, so it cannot be a ruler.
function done(_half: string): void {
  settled += 1;
  if (settled === 2) {
    clearTimeout(watchdog);
    server.close();
  }
}

// Half two, and it pins a DIFFERENT defect the same lot fixed: a refused
// connection reports `'error'` on the request. `node:http` schedules that report
// through `setTimeout(fn, 0)` from inside `node:net`'s delivery of the socket's
// error, and `entry::loops::pump_sources` asked `node:timers` BEFORE `node:net`,
// so the timer was created after the only pass that would have seen it, the host
// ended the program, and the listener ran for nothing. Node 22 answers
// `ECONNREFUSED` here. Port 1 is the refusal: nothing listens on it, and a port
// nothing listens on is the only portable way to ask for one.
function refusal(): void {
  const bad = request({ host: "127.0.0.1", port: 1, path: "/", method: "GET" }, () => {
    throw new Error("port 1 answered a response");
  });
  bad.on("error", (error: any) => {
    if (error.code !== "ECONNREFUSED") {
      throw new Error("refusal reported " + error.code + ", wanted ECONNREFUSED");
    }
    console.log("claude-http-loopback-exchange: refused ECONNREFUSED");
    done("refusal");
  });
  bad.end();
}

function finish(body: string): void {
  const order = seen.join(",");
  const want = "listening,client:socket,server:request GET /hi,server:req-end,client:response 200,client:data,client:end";
  if (order !== want) {
    throw new Error("event sequence was [" + order + "], wanted [" + want + "]");
  }
  if (body !== "pong") {
    throw new Error("body was [" + body + "], wanted [pong]");
  }
  console.log("claude-http-loopback-exchange: " + order + " body=" + body);
  done("exchange");
}

server.listen(0, "127.0.0.1", () => {
  seen.push("listening");
  // `server.address()` is the reason the port is not hardcoded: a fixed port is
  // a fixture that fails when anything else on the machine holds it.
  const port = server.address().port;
  const req = request({ host: "127.0.0.1", port, path: "/hi", method: "GET" }, (res: any) => {
    seen.push("client:response " + res.statusCode);
    let body = "";
    res.on("data", (chunk: any) => {
      body += chunk.toString();
      seen.push("client:data");
    });
    res.on("end", () => {
      seen.push("client:end");
      finish(body);
    });
  });
  req.on("socket", () => seen.push("client:socket"));
  req.on("error", (error: any) => {
    clearTimeout(watchdog);
    server.close();
    throw new Error("request errored: " + error.code + "; saw [" + seen.join(",") + "]");
  });
  req.end();
  refusal();
});

describe("http over loopback completes in one program", () => {
  test("the server reports the port the client has to dial", () => {
    expect(typeof server.address).toBe("function");
  });
});
