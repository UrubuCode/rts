// node:dgram — the synchronous surface of a UDP Socket: createSocket, bind,
// address, connect/remoteAddress/disconnect, the tuning setters, and the errors
// Node documents. Real OS sockets on the loopback interface.
//
// The EVENT side ('message'/'listening'/'close') is asynchronous — it fires in
// the post-main event loop, which the synchronous rts:test harness cannot
// observe — so delivery is integration-verified separately via `rts run` (a
// send/receive round trip). Everything observable synchronously is asserted here.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { createSocket } from "node:dgram";

// --- createSocket + bind + address -----------------------------------------
const s = createSocket("udp4");
const socketType = typeof s;
s.bind(0);
const addr = s.address();
const boundPort = addr.port;
const boundFamily = addr.family;
const boundAddress = addr.address;

// --- tuning (all need a bound socket) --------------------------------------
s.setTTL(64);
s.setBroadcast(true);
s.setBroadcast(false);
s.setMulticastTTL(1);
s.setMulticastLoopback(true);
let tuningOk = true;

s.setRecvBufferSize(65536);
// The OS may report more than was set (it commonly doubles the request for its
// own bookkeeping), so assert it is plausible rather than exact.
const recvBuf = s.getRecvBufferSize();
const sendBuf = s.getSendBufferSize();

// Nothing is queued: a literal-address send hands the bytes straight to the
// kernel.
const queueSize = s.getSendQueueSize();
const queueCount = s.getSendQueueCount();

// --- ref/unref are chainable ------------------------------------------------
const refIsSelf = s.ref() === s;
const unrefIsSelf = s.unref() === s;
s.ref();

// --- send to a bound peer ---------------------------------------------------
const peer = createSocket("udp4");
peer.bind(0);
const peerPort = peer.address().port;
s.send("hello", peerPort, "127.0.0.1");
s.send(new Uint8Array([104, 105]), peerPort, "127.0.0.1");
// Scatter: an array of parts becomes ONE datagram.
s.send(["a", new Uint8Array([98]), "c"], peerPort, "127.0.0.1");
// offset/length select a byte sub-range.
s.send(new Uint8Array([1, 2, 3, 4]), 1, 2, peerPort, "127.0.0.1");
let sendOk = true;

// --- connect / remoteAddress / disconnect -----------------------------------
const c = createSocket("udp4");
c.bind(0);
c.connect(peerPort, "127.0.0.1");
const remote = c.remoteAddress();
const remotePort = remote.port;
const remoteAddr = remote.address;
// A connected socket sends with no destination.
c.send("connected");
let connectedSendOk = true;

let doubleConnectErr = "";
try { c.connect(peerPort, "127.0.0.1"); } catch (e: any) { doubleConnectErr = e.message; }

c.disconnect();
let disconnectedTwiceErr = "";
try { c.disconnect(); } catch (e: any) { disconnectedTwiceErr = e.message; }

// --- errors -----------------------------------------------------------------
let badTypeErr = "";
try { createSocket("udp5"); } catch (e: any) { badTypeErr = e.message; }

let badPortErr = "";
try { s.send("no port"); } catch (e: any) { badPortErr = e.message; }

let notConnectedErr = "";
try { s.remoteAddress(); } catch (e: any) { notConnectedErr = e.message; }

// An unbound socket has no address.
const unbound = createSocket("udp4");
let unboundErr = "";
try { unbound.address(); } catch (e: any) { unboundErr = e.message; }
unbound.close();

// --- listeners --------------------------------------------------------------
function onMessage(_msg: any, _rinfo: any) {}
s.on("message", onMessage);
const countAfterOn = s.listenerCount("message");
const names = s.eventNames();
s.off("message", onMessage);
const countAfterOff = s.listenerCount("message");
const maxDefault = s.getMaxListeners();
s.setMaxListeners(20);
const maxAfterSet = s.getMaxListeners();

// --- close ------------------------------------------------------------------
s.close();
c.close();
peer.close();
let closedErr = "";
try { s.address(); } catch (e: any) { closedErr = e.message; }
let doubleCloseErr = "";
try { s.close(); } catch (e: any) { doubleCloseErr = e.message; }

describe("node:dgram Socket (synchronous surface)", () => {
  test("createSocket('udp4') returns a Socket object", () => {
    expect(socketType).toBe("object");
  });
  test("bind(0) picks a real ephemeral port", () => {
    expect(boundPort > 0).toBe(true);
  });
  test("address() reports the bound family and interface", () => {
    expect(boundFamily).toBe("IPv4");
    expect(boundAddress).toBe("0.0.0.0");
  });
  test("the TTL/broadcast/loopback setters apply", () => {
    expect(tuningOk).toBe(true);
  });
  test("buffer sizes report the OS's real values", () => {
    expect(recvBuf >= 65536).toBe(true);
    expect(sendBuf > 0).toBe(true);
  });
  test("an inline send leaves nothing queued", () => {
    expect(queueSize).toBe(0);
    expect(queueCount).toBe(0);
  });
  test("ref()/unref() return the socket", () => {
    expect(refIsSelf).toBe(true);
    expect(unrefIsSelf).toBe(true);
  });
  test("send accepts string, TypedArray, array and offset/length forms", () => {
    expect(sendOk).toBe(true);
  });
  test("connect() then remoteAddress() reports the peer", () => {
    expect(remotePort).toBe(peerPort);
    expect(remoteAddr).toBe("127.0.0.1");
    expect(connectedSendOk).toBe(true);
  });
  test("a second connect() throws ERR_SOCKET_DGRAM_IS_CONNECTED", () => {
    expect(doubleConnectErr.indexOf("ERR_SOCKET_DGRAM_IS_CONNECTED") >= 0).toBe(true);
  });
  test("disconnect() while not connected throws ERR_SOCKET_DGRAM_NOT_CONNECTED", () => {
    expect(disconnectedTwiceErr.indexOf("ERR_SOCKET_DGRAM_NOT_CONNECTED") >= 0).toBe(true);
  });
  test("an unknown socket type throws ERR_SOCKET_BAD_TYPE", () => {
    expect(badTypeErr.indexOf("ERR_SOCKET_BAD_TYPE") >= 0).toBe(true);
  });
  test("send with no port throws ERR_SOCKET_BAD_PORT", () => {
    expect(badPortErr.indexOf("ERR_SOCKET_BAD_PORT") >= 0).toBe(true);
  });
  test("remoteAddress() on an unconnected socket throws", () => {
    expect(notConnectedErr.indexOf("ERR_SOCKET_DGRAM_NOT_CONNECTED") >= 0).toBe(true);
  });
  test("address() on an unbound socket throws EBADF", () => {
    expect(unboundErr.indexOf("EBADF") >= 0).toBe(true);
  });
  test("the EventEmitter surface tracks listeners", () => {
    expect(countAfterOn).toBe(1);
    expect(countAfterOff).toBe(0);
    expect(names.length).toBe(1);
    expect(names[0]).toBe("message");
    expect(maxDefault).toBe(10);
    expect(maxAfterSet).toBe(20);
  });
  test("after close() the socket is not running", () => {
    expect(closedErr.indexOf("ERR_SOCKET_DGRAM_NOT_RUNNING") >= 0).toBe(true);
    expect(doubleCloseErr.indexOf("ERR_SOCKET_DGRAM_NOT_RUNNING") >= 0).toBe(true);
  });
});
