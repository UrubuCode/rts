// node:net — the synchronous surface of Server and Socket over real TCP.
//
// The EVENT side ('connection'/'data'/'close') is asynchronous — it fires in the
// post-main event loop, which the synchronous rts:test harness cannot observe —
// so a full echo round trip is `rts run`-verified separately (server.listen →
// connection → data both ways → close, with bytesRead/bytesWritten). What is
// asserted here is everything observable synchronously.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import {
  createServer,
  connect,
  createConnection,
  Socket,
  getDefaultAutoSelectFamily,
  setDefaultAutoSelectFamily,
  getDefaultAutoSelectFamilyAttemptTimeout,
  setDefaultAutoSelectFamilyAttemptTimeout,
} from "node:net";
import { BlockList } from "node:net";
import { time } from "rts";

// --- createServer + listen + address ---------------------------------------
const server = createServer();
const serverType = typeof server;
const addrBeforeListen = server.address();
const listeningBefore = server.listening;

server.listen(0, "127.0.0.1");
// `listen()` is asynchronous in real Node too — verified directly:
// `server.listen(0, host); server.address()` reads `null` right after the
// call, same as here (see `engine_object_backed_props.test.ts`'s own note on
// the same check). This file used to read `address()`/`listening` with no
// wait and asserted them populated, which is not what Node does either;
// `time.sleep_ms` is this suite's quiescence point (`net_tcp_echo.test.ts`'s
// own `thread.sleep_ms`), giving the accept thread time to bind and queue
// the `'listening'` event before either is read.
time.sleep_ms(50);
const addr = server.address();
const boundPort = addr.port;
const boundFamily = addr.family;
const boundAddress = addr.address;
const listeningAfter = server.listening;

// listen() twice without closing throws.
let doubleListenErr = "";
try { server.listen(0); } catch (e: any) { doubleListenErr = e.message; }

// `maxConnections`/`dropMaxConnection` are PLAIN properties in Node (not
// accessors) — the accept thread reads them back off the JS object. The limit
// itself is enforced asynchronously (see the `'drop'` note at the top), so what
// is asserted here is that the write lands and round-trips by value.
server.maxConnections = 2;
const maxConnRead = server.maxConnections;
const maxConnType = typeof server.maxConnections;
server.maxConnections = 1;
const maxConnRewritten = server.maxConnections;
server.dropMaxConnection = true;
const dropMaxRead = server.dropMaxConnection;
const dropMaxType = typeof server.dropMaxConnection;
// An unset property is still undefined — a write is not a shape transition that
// invents keys.
const unsetProp = (server as any).__nothing_wrote_this;

// ref/unref are chainable.
const serverRefIsSelf = server.ref() === server;
const serverUnrefIsSelf = server.unref() === server;
server.ref();

// --- a second server on the same fixed port errors, not throws --------------
// (EADDRINUSE arrives as an 'error' EVENT — a Server, unlike a Socket, is NOT
// auto-closed after one.)
const clash = createServer();
function onClashError(_e: any) {}
clash.on("error", onClashError);
clash.listen(boundPort, "127.0.0.1");
let clashDidNotThrow = true;

// --- new Socket + its pre-connect state -------------------------------------
//
// Verified directly against a real Node (v25): a fresh, never-connected
// `Socket`'s `readyState` reads `"open"` (readable/writable both default
// `true`, and `readyState` is derived from those two, never from
// `connecting`/`pending`), `bufferSize`/`timeout`/
// `autoSelectFamilyAttemptedAddresses` all read `undefined` (no internal
// buffer/timer/attempt-list exists before a connection is even attempted).
// This block used to assert `"closed"`, `0`, `-1` and `[]` respectively,
// none of which is what Node answers; corrected to what it actually does.
const sock = new Socket();
const sockType = typeof sock;
const pendingBefore = sock.pending;
const connectingBefore = sock.connecting;
const destroyedBefore = sock.destroyed;
const readyStateBefore = sock.readyState;
const bytesReadBefore = sock.bytesRead;
const bufferSizeBefore = sock.bufferSize;
const attemptedBefore = sock.autoSelectFamilyAttemptedAddresses;

// setEncoding/pause/resume/setNoDelay/setKeepAlive/setTimeout are chainable and
// safe before connecting.
const chainOk = sock.setEncoding("utf8") === sock
  && sock.pause() === sock
  && sock.resume() === sock
  && sock.setTimeout(0) === sock
  && sock.ref() === sock;
// `setTimeout(0)` above already set it to `0` — real Node reads `0` here,
// not `-1` (an unset timer, before any `setTimeout` call at all, is
// `undefined`, not `-1` either; this fixture never observes that state).
const timeoutUnset = sock.timeout;
sock.setTimeout(1000);
const timeoutSet = sock.timeout;

// --- connect + createConnection return a Socket -----------------------------
const client = connect(boundPort, "127.0.0.1");
const clientType = typeof client;
const clientConnecting = client.connecting;
const client2 = createConnection(boundPort, "127.0.0.1");
const client2Type = typeof client2;

// --- errors -----------------------------------------------------------------
let badPortErr = "";
try { new Socket().connect(70000); } catch (e: any) { badPortErr = e.message; }

let missingArgsErr = "";
try { new Socket().connect("localhost"); } catch (e: any) { missingArgsErr = e.message; }

// An unimplemented option is refused, not ignored.
let ipcErr = "";
try { createServer({ path: "/tmp/x.sock" }); } catch (e: any) { ipcErr = e.message; }

let fdErr = "";
try { new Socket({ fd: 3 }); } catch (e: any) { fdErr = e.message; }

// A blockList that is not a net.BlockList is rejected; a real one is accepted.
let badBlockListErr = "";
try { createServer({ blockList: {} }); } catch (e: any) { badBlockListErr = e.message; }
const guarded = createServer({ blockList: new BlockList() });
const guardedType = typeof guarded;

// --- module-level autoSelectFamily config -----------------------------------
const asfDefault = getDefaultAutoSelectFamily();
setDefaultAutoSelectFamily(false);
const asfOff = getDefaultAutoSelectFamily();
setDefaultAutoSelectFamily(asfDefault);

const asfTimeoutDefault = getDefaultAutoSelectFamilyAttemptTimeout();
setDefaultAutoSelectFamilyAttemptTimeout(5);
// Node clamps values < 10 up to 10.
const asfTimeoutClamped = getDefaultAutoSelectFamilyAttemptTimeout();
setDefaultAutoSelectFamilyAttemptTimeout(asfTimeoutDefault);

// --- close ------------------------------------------------------------------
client.destroy();
client2.destroy();
sock.destroy();
const destroyedAfter = sock.destroyed;
server.close();
clash.close();
const addrAfterClose = server.address();

describe("node:net Server", () => {
  test("createServer returns a Server", () => {
    expect(serverType).toBe("object");
    expect(addrBeforeListen === null || addrBeforeListen === undefined).toBe(true);
    expect(listeningBefore).toBe(false);
  });
  test("listen(0, host) binds a real ephemeral port", () => {
    expect(boundPort > 0).toBe(true);
    expect(boundFamily).toBe("IPv4");
    expect(boundAddress).toBe("127.0.0.1");
    expect(listeningAfter).toBe(true);
  });
  test("a second listen() without closing throws ERR_SERVER_ALREADY_LISTEN", () => {
    expect(doubleListenErr.indexOf("ERR_SERVER_ALREADY_LISTEN") >= 0).toBe(true);
  });
  test("ref()/unref() return the server", () => {
    expect(serverRefIsSelf).toBe(true);
    expect(serverUnrefIsSelf).toBe(true);
  });
  test("maxConnections/dropMaxConnection are writable plain properties", () => {
    expect(maxConnRead).toBe(2);
    expect(maxConnType).toBe("number");
    expect(maxConnRewritten).toBe(1);
    expect(dropMaxRead).toBe(true);
    expect(dropMaxType).toBe("boolean");
    expect(unsetProp).toBe(undefined);
  });
  test("a port clash reports asynchronously, it does not throw", () => {
    expect(clashDidNotThrow).toBe(true);
  });
  test("address() is null after close()", () => {
    expect(addrAfterClose === null || addrAfterClose === undefined).toBe(true);
  });
  test("a blockList option takes a real net.BlockList and refuses anything else", () => {
    expect(guardedType).toBe("object");
    expect(badBlockListErr.indexOf("ERR_INVALID_ARG_TYPE") >= 0).toBe(true);
  });
  test("the unimplemented IPC path option is refused, not ignored", () => {
    expect(ipcErr.indexOf("ERR_INVALID_ARG_VALUE") >= 0).toBe(true);
  });
});

describe("node:net Socket", () => {
  test("new Socket() starts pending, not connecting, not destroyed", () => {
    expect(sockType).toBe("object");
    expect(pendingBefore).toBe(true);
    expect(connectingBefore).toBe(false);
    expect(destroyedBefore).toBe(false);
    expect(readyStateBefore).toBe("open");
    expect(bytesReadBefore).toBe(0);
    expect(bufferSizeBefore).toBe(undefined);
    expect(attemptedBefore).toBe(undefined);
  });
  test("the tuning setters are chainable", () => {
    expect(chainOk).toBe(true);
  });
  test("setTimeout round-trips through the timeout property", () => {
    expect(timeoutUnset).toBe(0);
    expect(timeoutSet).toBe(1000);
  });
  test("connect/createConnection return a connecting Socket", () => {
    expect(clientType).toBe("object");
    expect(client2Type).toBe("object");
    expect(clientConnecting).toBe(true);
  });
  test("a bad port throws ERR_SOCKET_BAD_PORT", () => {
    expect(badPortErr.indexOf("ERR_SOCKET_BAD_PORT") >= 0).toBe(true);
  });
  test("connect with no port throws ERR_MISSING_ARGS", () => {
    expect(missingArgsErr.indexOf("ERR_MISSING_ARGS") >= 0).toBe(true);
  });
  test("the unimplemented fd option is refused, not ignored", () => {
    expect(fdErr.indexOf("ERR_INVALID_ARG_VALUE") >= 0).toBe(true);
  });
  test("destroy() marks the socket destroyed", () => {
    expect(destroyedAfter).toBe(true);
  });
});

describe("node:net autoSelectFamily config", () => {
  test("the default is on, and it is process-wide settable", () => {
    expect(asfDefault).toBe(true);
    expect(asfOff).toBe(false);
  });
  test("an attempt timeout below 10 clamps up to 10", () => {
    expect(asfTimeoutDefault).toBe(250);
    expect(asfTimeoutClamped).toBe(10);
  });
});
