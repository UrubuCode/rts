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

// --- createServer + listen + address ---------------------------------------
const server = createServer();
const serverType = typeof server;
const addrBeforeListen = server.address();
const listeningBefore = server.listening;

server.listen(0, "127.0.0.1");
const addr = server.address();
const boundPort = addr.port;
const boundFamily = addr.family;
const boundAddress = addr.address;
const listeningAfter = server.listening;

// listen() twice without closing throws.
let doubleListenErr = "";
try { server.listen(0); } catch (e: any) { doubleListenErr = e.message; }

// NOT asserted: `server.maxConnections = n`. It is a plain property in Node and
// the accept thread reads it off the object — but the engine does not yet write
// a property onto an object-backed class instance, so the assignment does not
// land. Recorded as a known gap in net.md §8 rather than asserted as working.

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
    expect(readyStateBefore).toBe("closed");
    expect(bytesReadBefore).toBe(0);
    expect(bufferSizeBefore).toBe(0);
    expect(attemptedBefore.length).toBe(0);
  });
  test("the tuning setters are chainable", () => {
    expect(chainOk).toBe(true);
  });
  test("setTimeout round-trips through the timeout property", () => {
    expect(timeoutUnset).toBe(-1);
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
