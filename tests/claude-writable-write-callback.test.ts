import { describe, test, expect } from "rts:test";
import { Writable, Duplex } from "node:stream";

// `writable.write(chunk, callback)` — the two-argument form, where the SECOND
// argument is the completion callback and not an encoding — must fire that
// callback. Node's signature is `write(chunk[, encoding][, callback])`, so a
// function in the encoding slot is the callback.
//
// What this pins is the wall a real WhatsApp client hit. `ws`'s sender writes a
// frame with `socket.write(buffer, cb)`, and `@whiskeysockets/baileys` wraps
// `ws.send` in `util.promisify`. With the callback dropped, that promise never
// settles, so `await sendRawMessage(clientFinish)` never returns and
// `await noise.finishInit()` on the next line never runs — the Noise TRANSPORT
// cipher is never installed. The server's answer then arrives, is decrypted by
// nothing, and `decodeFrame` hands the application raw bytes instead of the
// `iq`/`pair-device` node that carries the QR code. Twenty seconds later
// `promiseTimeout(connectTimeoutMs, …)` answers 408 and the whole failure reads
// as "the server did not reply", which it did: measured 698 bytes in, byte for
// byte what Node received.
//
// So the assertion to make is not "a socket works". It is that a dropped
// callback is observable at all — the engine's own answer was correct in every
// other respect, and nothing in the send path, the frame path or the crypto was
// wrong.
//
// Every value below was measured against Node 22.23.2. No network and no
// `node_modules`: the defect is three calls on a `Writable`.
describe("Writable#write with a callback in the encoding slot", () => {
  test("write(chunk, callback) fires the callback", () => {
    const written: string[] = [];
    const fired: string[] = [];
    const w = new Writable({
      write(chunk: any, _encoding: any, done: any) {
        written.push(String(chunk));
        done();
      },
    });
    w.write("a", () => fired.push("two-argument"));
    expect(written).toEqual(["a"]);
    expect(fired).toEqual(["two-argument"]);
  });

  test("write(chunk, encoding, callback) keeps firing it", () => {
    const fired: string[] = [];
    const w = new Writable({
      write(_chunk: any, _encoding: any, done: any) {
        done();
      },
    });
    w.write("b", "utf8", () => fired.push("three-argument"));
    expect(fired).toEqual(["three-argument"]);
  });

  test("a function in the encoding slot is not treated as an encoding", () => {
    // `normalize_chunk` reads the encoding to decide whether to re-encode the
    // text. A callback mistaken for an encoding must not change the bytes the
    // `_write` hook is handed: Node answers the string unchanged.
    const seen: string[] = [];
    const w = new Writable({
      write(chunk: any, _encoding: any, done: any) {
        seen.push(String(chunk));
        done();
      },
    });
    w.write("hello", () => {});
    expect(seen).toEqual(["hello"]);
  });

  test("a Duplex inherits the same two-argument form", () => {
    // `net.Socket` and every `ws` socket are Duplexes, and the mixin that gives
    // `Duplex` its writable half is a copy of `Writable`'s method table — so the
    // fix has to be in the shared method, not in one class's own copy.
    const fired: string[] = [];
    const d = new Duplex({
      read() {},
      write(_chunk: any, _encoding: any, done: any) {
        done();
      },
    });
    d.write("c", () => fired.push("duplex"));
    expect(fired).toEqual(["duplex"]);
  });

  test("end(chunk, callback) still fires it", () => {
    // Already correct before this change — asserted so that the shift added to
    // `write` cannot be "fixed" later by moving the one `end` already had.
    const fired: string[] = [];
    const w = new Writable({
      write(_chunk: any, _encoding: any, done: any) {
        done();
      },
    });
    w.end("d", () => fired.push("end"));
    expect(fired).toEqual(["end"]);
  });
});
