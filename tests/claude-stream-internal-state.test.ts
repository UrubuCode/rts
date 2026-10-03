import { describe, test, expect } from "rts:test";
import { Readable, Writable, Duplex } from "node:stream";
import * as net from "node:net";

// `_readableState` / `_writableState` — the internal state objects real Node puts
// on every stream, and that library code reads straight off a socket.
//
// `ws` 8.22's `socketOnClose` reads `socket._readableState.endEmitted` and
// `.length`, then `receiver._writableState.errorEmitted` and `.finished`. With
// them absent, closing any WebSocket died with `Cannot read properties of
// undefined (reading 'endEmitted')` — at CLOSE, which is why a green stream suite
// said nothing: the socket had already done its whole job.
//
// Every field below is tracked under its public name already (`readableEnded`,
// `writableFinished`, …), so what is asserted here is the NAME and the agreement
// between the two spellings — not a new state machine. The view is computed per
// read, so a write through it is not observed; `state_view.rs` says why a stored
// second copy was rejected.
//
// Which side sits on which prototype was measured against Node 22.23.2: a
// `Readable` has no `_writableState`, a `Writable` has no `_readableState`, and a
// `Duplex` and a `net.Socket` carry both. No network: a never-connected socket
// answers every one of these.
describe("the internal state objects ws reads", () => {
  test("a Readable carries the readable half and not the writable one", () => {
    const r: any = new Readable({ read() {} });
    expect(typeof r._readableState).toBe("object");
    expect(r._readableState.endEmitted).toBe(false);
    expect(r._readableState.length).toBe(0);
    expect(r._readableState.objectMode).toBe(false);
    expect(r._readableState.highWaterMark).toBe(16384);
    expect(r._writableState).toBe(undefined);
  });

  test("a Writable carries the writable half and not the readable one", () => {
    const w: any = new Writable({
      write(_c: any, _e: any, done: any) {
        done();
      },
    });
    expect(typeof w._writableState).toBe("object");
    expect(w._writableState.finished).toBe(false);
    expect(w._writableState.errorEmitted).toBe(false);
    expect(w._writableState.ended).toBe(false);
    expect(w._writableState.length).toBe(0);
    expect(w._writableState.corked).toBe(0);
    expect(w._readableState).toBe(undefined);
  });

  test("a Duplex carries both halves", () => {
    const d: any = new Duplex({
      read() {},
      write(_c: any, _e: any, done: any) {
        done();
      },
    });
    expect(typeof d._readableState).toBe("object");
    expect(typeof d._writableState).toBe("object");
  });

  test("a net.Socket carries both halves", () => {
    // The exact read `ws`'s `socketOnClose` performs, on the exact class it gets
    // handed. A `tls.TLSSocket` inherits from this prototype.
    const s: any = new net.Socket();
    expect(s._readableState.endEmitted).toBe(false);
    expect(s._readableState.length).toBe(0);
    expect(s._writableState.errorEmitted).toBe(false);
    expect(s._writableState.finished).toBe(false);
  });

  test("the readable view follows the stream it describes", () => {
    const r: any = new Readable({ read() {} });
    r.push("abcd");
    expect(r._readableState.length).toBe(4);
    // `ended` is the push side (`push(null)` seen); `endEmitted` is the event.
    // Node keeps them apart, and collapsing them would make a close read as an
    // already-emitted end.
    expect(r._readableState.ended).toBe(false);
    r.push(null);
    expect(r._readableState.ended).toBe(true);
  });

  test("the writable view follows the stream it describes", () => {
    const w: any = new Writable({
      write(_c: any, _e: any, done: any) {
        done();
      },
    });
    w.end("x");
    expect(w._writableState.ended).toBe(true);
    // `finished` is NOT asserted here: Node answers `false` on the line after
    // `end()` and `true` a tick later, because its completion is deferred, while
    // this crate finishes a write synchronously (`mod.rs`'s doc states that
    // difference for `'data'` and it is the same one). Asserting `true` would be a
    // test that passes here and fails in the runtime being matched.
  });
});
