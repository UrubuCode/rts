# `ws` — WebSocket server

**Not a `node:` module, and the distinction is the point.** In Node, a WebSocket
*server* is not part of the platform. What the standard library provides is the
`'upgrade'` event on an `http.Server`, handing over the raw socket; the
handshake and the RFC 6455 framing come from the `ws` package on npm, which is
the de-facto reference implementation. A *client* is different — `WebSocket` has
been a global since v22 (stable v23), backed by undici.

So this is registered under the bare specifier `ws` and **only** that. Every
other module in `rts-node` is registered twice — `node:fs` and `fs` — because
those are two spellings of one Node module. `node:ws` would be a name the
platform does not have, and a program ported from Node would never look for it.

| | in Node | here |
|---|---|---|
| server | npm `ws` | `import { WebSocketServer } from "ws"` |
| client | `WebSocket` global | **not implemented** — see below |

## Surface

```js
import { WebSocketServer } from "ws";

const wss = new WebSocketServer({ port: 7788 });

wss.on("listening", () => { /* … */ });
wss.on("connection", (ws, req) => {
  // req.url, req.headers.host
  ws.on("message", (data, isBinary) => ws.send(data));
  ws.on("close", (code, reason) => { /* … */ });
  ws.on("error", (err) => { /* … */ });
});
```

Both classes inherit `EventEmitter` — the real one, from `node:events` — so
`once`, `off` and `removeAllListeners` work without this module knowing what
they are.

| name | shape |
|---|---|
| `new WebSocketServer({ port, host })` | `host` defaults to `0.0.0.0` |
| `wss.close()` | stops accepting |
| `ws.send(data)` | `string` → TEXT frame, typed array → BINARY frame |
| `ws.close(code, reason)` | `code` defaults to 1000 |
| `ws.readyState` | `1` open, `3` closed |
| `'message'` | `(data, isBinary)` — `data` is a `string` or a `Buffer` |
| `'close'` | `(code, reason)` |

`WebSocketServer`, `Server` and the default export are the **same** constructor
object, as in `ws` — `a.Server === a.WebSocketServer` holds.

## A listening server does not keep the program alive; a connection does

This follows a decision `rts_core::entry::loops` documents: a source that answers
`Pending::Blocked` is pumped but does not hold the loop open, so a listening
server would otherwise run forever and no test could finish. A program that only
listens therefore needs something else to hold it — a timer is the usual answer.

An **open connection** is different: it answers a deadline, so the loop keeps
waking to look at it, and that does hold the program open. This matches Node,
where a connected socket keeps the process alive, and it is not cosmetic —
without it the loop sleeps until the next timer and messages that arrive
meanwhile are delivered late or never.

## Not implemented, by name

| missing | why |
|---|---|
| `new WebSocketServer({ server })`, `{ noServer: true }` | need `node:http`'s `'upgrade'` event, which this runtime does not emit (see `http.md`) |
| **client** — `new WebSocket(url)` | the core serves both sides (`conn::adopt` takes whether this side masks); the outbound handshake is what is missing |
| `permessage-deflate` | the extension is never negotiated, so the server never claims to support it |
| `ws.ping()` / `ws.pong()` | an *incoming* PING **is** answered with a PONG — that is an RFC obligation and it is implemented. What is missing is the program being able to send one. |
| `binaryType`, `bufferedAmount`, `Sec-WebSocket-Protocol` | — |
| `wss.clients` | — |

The `WebSocket` global (client) is tracked separately in `globals.md` §2.19,
which lists it as deferred. When it lands it comes from this same core:
`ws/frame.rs` knows nothing about client or server, only the RFC.

## What is pinned by a test, and what is not

**Pinned** (`cargo test -p rts-node --lib ws::`, 11 tests): frame reading at
every split point, masking round-trip, fragmented BINARY not becoming text, TEXT
that is not UTF-8 being refused rather than corrupted, oversized control frames
and absurd declared lengths refused at the header, and the RFC's own
`Sec-WebSocket-Accept` example vector.

**Not pinned by an automated test, and verified by hand instead:** the end-to-end
path. A server was run under `run_fixture` and driven by Python's `websockets` —
an independent implementation, deliberately, since our own client would repeat
our own mistakes. Text echo, binary echo as a `Buffer`, and a close with code
`1000` and reason `tchau` all round-tripped. Automating it needs either a client
in this runtime (which does not exist yet) or a Rust integration test that speaks
the protocol against a running program.

**Conformance is ours now.** Writing the protocol by hand rather than taking
`tungstenite` was a deliberate choice; the cost is that nothing external checks
us. The Autobahn test suite is what would, and running it is the honest next
step before anyone calls this conformant.
