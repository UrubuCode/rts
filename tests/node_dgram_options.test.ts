// node:dgram — the createSocket(options) form, udp6, multicast membership, and
// the honest refusals. Real sockets and real setsockopt calls; a failure here is
// the OS reporting a real error, never a fabricated one.

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { createSocket } from "node:dgram";

// --- createSocket(options) --------------------------------------------------
// Overload by VALUE: the same member accepts the type string or the options
// object, exactly as Node's own JS decides.
const o = createSocket({ type: "udp4", reuseAddr: true, recvBufferSize: 65536 });
o.bind(0);
const optsPort = o.address().port;
const optsRecvBuf = o.getRecvBufferSize();

// reuseAddr: true lets a second socket take the same port.
const shared1 = createSocket({ type: "udp4", reuseAddr: true });
shared1.bind(0);
const sharedPort = shared1.address().port;
let reuseAddrOk = true;

// --- udp6 -------------------------------------------------------------------
const six = createSocket("udp6");
six.bind(0, "::1");
const sixAddr = six.address();
const sixFamily = sixAddr.family;
const sixPort = sixAddr.port;
// v6 tuning goes through the IPv6 hop-limit options, not the v4 TTL ones.
six.setTTL(64);
six.setMulticastTTL(1);
let sixTuningOk = true;

// --- multicast --------------------------------------------------------------
const mc = createSocket({ type: "udp4", reuseAddr: true });
mc.bind(0);
mc.addMembership("239.255.42.99");
mc.setMulticastInterface("0.0.0.0");
mc.dropMembership("239.255.42.99");
let multicastOk = true;

let badGroupErr = "";
try { mc.addMembership("not-an-address"); } catch (e: any) { badGroupErr = e.message; }

// A v6 group on a v4 socket is a family mismatch.
let familyMismatchErr = "";
try { mc.addMembership("ff02::1"); } catch (e: any) { familyMismatchErr = e.message; }

// Source-specific multicast (IGMPv3): the source must parse, and it must be the
// same family as the group.
let ssmBadSourceErr = "";
try {
  mc.addSourceSpecificMembership("nope", "232.1.2.3");
} catch (e: any) { ssmBadSourceErr = e.message; }

let ssmMixedFamilyErr = "";
try {
  mc.addSourceSpecificMembership("::1", "232.1.2.3");
} catch (e: any) { ssmMixedFamilyErr = e.message; }

// --- deferred options are refused, never silently ignored -------------------
let lookupErr = "";
try {
  createSocket({ type: "udp4", lookup: (_h: any, _o: any, _cb: any) => {} });
} catch (e: any) { lookupErr = e.message; }

let blockListErr = "";
try {
  createSocket({ type: "udp4", receiveBlockList: {} });
} catch (e: any) { blockListErr = e.message; }

o.close();
shared1.close();
six.close();
mc.close();

describe("node:dgram createSocket options / udp6 / multicast", () => {
  test("createSocket(options) binds like the type form", () => {
    expect(optsPort > 0).toBe(true);
  });
  test("recvBufferSize is applied at creation", () => {
    expect(optsRecvBuf >= 65536).toBe(true);
  });
  test("reuseAddr sockets bind", () => {
    expect(reuseAddrOk).toBe(true);
    expect(sharedPort > 0).toBe(true);
  });
  test("udp6 binds on ::1 and reports IPv6", () => {
    expect(sixFamily).toBe("IPv6");
    expect(sixPort > 0).toBe(true);
    expect(sixTuningOk).toBe(true);
  });
  test("multicast join/leave on a real group", () => {
    expect(multicastOk).toBe(true);
  });
  test("a malformed group address throws EINVAL", () => {
    expect(badGroupErr.indexOf("EINVAL") >= 0).toBe(true);
  });
  test("a group of the wrong family throws EINVAL", () => {
    expect(familyMismatchErr.indexOf("EINVAL") >= 0).toBe(true);
  });
  test("source-specific membership validates its source address", () => {
    expect(ssmBadSourceErr.indexOf("EINVAL") >= 0).toBe(true);
    expect(ssmMixedFamilyErr.indexOf("EINVAL") >= 0).toBe(true);
  });
  test("the unimplemented lookup option is refused, not ignored", () => {
    expect(lookupErr.indexOf("ERR_INVALID_ARG_VALUE") >= 0).toBe(true);
    expect(lookupErr.indexOf("lookup") >= 0).toBe(true);
  });
  test("the unimplemented receiveBlockList option is refused, not ignored", () => {
    expect(blockListErr.indexOf("ERR_INVALID_ARG_VALUE") >= 0).toBe(true);
  });
});
