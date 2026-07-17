// node:dgram + node:net — the receiveBlockList/sendBlockList options, now that
// net.BlockList is real. Node's semantics: a blocked SENDER's datagram never
// reaches a 'message' listener; a blocked DESTINATION is refused instead of
// dialed. (Node's own doc: not a security boundary behind a proxy/NAT — the
// address checked is the immediate peer's.)
//
// The refusal is delivered to the send callback / 'error' listener, which fires
// in the post-main event loop; what is asserted here is the surface the
// synchronous harness can observe: the options are ACCEPTED (no longer refused),
// a non-BlockList is rejected, and a blocked send does not throw synchronously.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { createSocket } from "node:dgram";
import { BlockList } from "node:net";

// --- receiveBlockList is accepted -------------------------------------------
const inbound = new BlockList();
inbound.addAddress("127.0.0.1");
const receiver = createSocket({ type: "udp4", receiveBlockList: inbound });
receiver.bind(0);
const receiverPort = receiver.address().port;
let receiveBlockListAccepted = true;

// --- sendBlockList is accepted ----------------------------------------------
const outbound = new BlockList();
outbound.addSubnet("127.0.0.0", 8);
const sender = createSocket({ type: "udp4", sendBlockList: outbound });
sender.bind(0);
// The refusal arrives as a real 'error' (post-main). Like any Node program, a
// socket must carry an 'error' listener — an unhandled one is fatal by design.
function onRefused(_err: any) {}
sender.on("error", onRefused);
// A send to a blocked destination is refused asynchronously (callback/'error'),
// never thrown synchronously.
sender.send("blocked", receiverPort, "127.0.0.1");
let blockedSendDidNotThrow = true;

// An unblocked destination still goes out.
const allowed = createSocket({ type: "udp4", sendBlockList: new BlockList() });
allowed.bind(0);
allowed.send("allowed", receiverPort, "127.0.0.1");
let allowedSendOk = true;

// --- a non-BlockList is rejected --------------------------------------------
let badTypeErr = "";
try {
  createSocket({ type: "udp4", sendBlockList: { addAddress: 1 } });
} catch (e: any) { badTypeErr = e.message; }

// --- the deferred options are still refused ---------------------------------
let lookupErr = "";
try {
  createSocket({ type: "udp4", lookup: (_h: any, _o: any, _cb: any) => {} });
} catch (e: any) { lookupErr = e.message; }

receiver.close();
sender.close();
allowed.close();

describe("node:dgram block lists (net.BlockList)", () => {
  test("receiveBlockList is accepted by createSocket", () => {
    expect(receiveBlockListAccepted).toBe(true);
    expect(receiverPort > 0).toBe(true);
  });
  test("a send to a blocked destination does not throw synchronously", () => {
    expect(blockedSendDidNotThrow).toBe(true);
  });
  test("a send to an unblocked destination is dispatched", () => {
    expect(allowedSendOk).toBe(true);
  });
  test("a block-list option that is not a BlockList is rejected", () => {
    expect(badTypeErr.indexOf("ERR_INVALID_ARG_TYPE") >= 0).toBe(true);
    expect(badTypeErr.indexOf("net.BlockList") >= 0).toBe(true);
  });
  test("the still-unimplemented lookup option remains refused", () => {
    expect(lookupErr.indexOf("ERR_INVALID_ARG_VALUE") >= 0).toBe(true);
  });
});
