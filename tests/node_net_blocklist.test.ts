// node:net — BlockList + SocketAddress. Pure logic, matching Node's own
// semantics from src/node_sockaddr.cc: rules match ACROSS families (an IPv4
// rule covers the IPv4-mapped IPv6 form and vice versa), ranges compare
// numerically, and the rule strings are API (toJSON emits them, fromJSON reads
// them back).

let __out: string[] = [];
function print(s: string) { __out.push(s); }

import { describe, test, expect } from "rts:test";
import { BlockList, SocketAddress, isIP, isIPv4, isIPv6 } from "node:net";

// --- isIP family ------------------------------------------------------------
const ip4 = isIP("127.0.0.1");
const ip6 = isIP("::1");
const ipNo = isIP("not-an-ip");
const v4Yes = isIPv4("192.168.1.1");
const v4No = isIPv4("::1");
const v6Yes = isIPv6("::1");
const v6No = isIPv6("192.168.1.1");

// --- BlockList: addAddress --------------------------------------------------
const bl = new BlockList();
bl.addAddress("1.2.3.4");
const hitExact = bl.check("1.2.3.4");
const missExact = bl.check("1.2.3.5");

// An IPv4 rule covers the IPv4-MAPPED IPv6 form (Node's cross-family match).
const hitMapped = bl.check("::ffff:1.2.3.4", "ipv6");
const missMapped = bl.check("::ffff:1.2.3.5", "ipv6");
// A real (non-mapped) IPv6 address never matches an IPv4 rule.
const missRealV6 = bl.check("2001:db8::1", "ipv6");

// --- BlockList: addRange ----------------------------------------------------
const range = new BlockList();
range.addRange("1.2.3.0", "1.2.3.255");
const inRange = range.check("1.2.3.128");
const atStart = range.check("1.2.3.0");
const atEnd = range.check("1.2.3.255");
const belowRange = range.check("1.2.2.255");
const aboveRange = range.check("1.2.4.0");

// --- BlockList: addSubnet ---------------------------------------------------
const subnet = new BlockList();
subnet.addSubnet("10.0.0.0", 8);
const inSubnet = subnet.check("10.1.2.3");
const outSubnet = subnet.check("11.0.0.1");

// A /32 is a single host; a /0 covers everything.
const host32 = new BlockList();
host32.addSubnet("192.168.1.7", 32);
const hit32 = host32.check("192.168.1.7");
const miss32 = host32.check("192.168.1.8");

const all0 = new BlockList();
all0.addSubnet("0.0.0.0", 0);
const hitAny = all0.check("203.0.113.9");

// --- BlockList: IPv6 --------------------------------------------------------
const v6 = new BlockList();
v6.addAddress("::1", "ipv6");
v6.addSubnet("2001:db8::", 32, "ipv6");
const hitV6Addr = v6.check("::1", "ipv6");
const hitV6Subnet = v6.check("2001:db8:1234::5", "ipv6");
const missV6Subnet = v6.check("2001:db9::1", "ipv6");
// A prefix that is not byte-aligned exercises the partial-byte mask.
const v6odd = new BlockList();
v6odd.addSubnet("2001:db8::", 33, "ipv6");
const hitOdd = v6odd.check("2001:db8:7fff::1", "ipv6");
const missOdd = v6odd.check("2001:db8:8000::1", "ipv6");

// --- rules / toJSON / fromJSON ---------------------------------------------
const ruled = new BlockList();
ruled.addAddress("1.2.3.4");
ruled.addRange("10.0.0.1", "10.0.0.9");
ruled.addSubnet("172.16.0.0", 12);
ruled.addAddress("::1", "ipv6");
const rules = ruled.rules;
const ruleCount = rules.length;
const rule0 = rules[0];
const rule1 = rules[1];
const rule2 = rules[2];
const rule3 = rules[3];

// toJSON round-trips through a fresh BlockList.
const json = ruled.toJSON();
const restored = new BlockList();
restored.fromJSON(json);
const restoredCount = restored.rules.length;
const restoredHitAddress = restored.check("1.2.3.4");
const restoredHitRange = restored.check("10.0.0.5");
const restoredHitSubnet = restored.check("172.16.5.5");
const restoredHitV6 = restored.check("::1", "ipv6");

// --- isBlockList ------------------------------------------------------------
const isBl = BlockList.isBlockList(bl);
const isNotBl = BlockList.isBlockList(new SocketAddress());

// --- errors -----------------------------------------------------------------
let badPrefixErr = "";
try { new BlockList().addSubnet("10.0.0.0", 200); } catch (e: any) { badPrefixErr = e.message; }

let badAddrErr = "";
try { new BlockList().addAddress("nope"); } catch (e: any) { badAddrErr = e.message; }

// An IPv6 literal declared as 'ipv4' is not a valid ipv4 address.
let familyErr = "";
try { new BlockList().addAddress("::1"); } catch (e: any) { familyErr = e.message; }

let invertedErr = "";
try { new BlockList().addRange("1.2.3.9", "1.2.3.1"); } catch (e: any) { invertedErr = e.message; }

// check() never throws — an unparseable address is simply not in the list.
const checkGarbage = bl.check("garbage");

// --- SocketAddress ----------------------------------------------------------
const sa = new SocketAddress();
const saAddress = sa.address;
const saFamily = sa.family;
const saPort = sa.port;
const saFlow = sa.flowlabel;

const sa6 = new SocketAddress({ family: "ipv6" });
const sa6Address = sa6.address;
const sa6Family = sa6.family;

const saOpts = new SocketAddress({ address: "192.168.1.9", port: 8080 });
const saOptsAddress = saOpts.address;
const saOptsPort = saOpts.port;

const parsed4 = SocketAddress.parse("123.1.2.3:1234");
const parsed4Address = parsed4.address;
const parsed4Port = parsed4.port;
const parsed4Family = parsed4.family;

const parsed6 = SocketAddress.parse("[1::1]:1234");
const parsed6Address = parsed6.address;
const parsed6Port = parsed6.port;
const parsed6Family = parsed6.family;

const parsedBad = SocketAddress.parse("garbage");

// A SocketAddress is accepted anywhere Node takes `string | SocketAddress`.
const blSa = new BlockList();
blSa.addAddress(new SocketAddress({ address: "8.8.8.8" }));
const hitViaSocketAddress = blSa.check("8.8.8.8");

describe("node:net isIP", () => {
  test("classifies IPv4/IPv6/garbage", () => {
    expect(ip4).toBe(4);
    expect(ip6).toBe(6);
    expect(ipNo).toBe(0);
    expect(v4Yes).toBe(true);
    expect(v4No).toBe(false);
    expect(v6Yes).toBe(true);
    expect(v6No).toBe(false);
  });
});

describe("node:net BlockList", () => {
  test("addAddress matches exactly", () => {
    expect(hitExact).toBe(true);
    expect(missExact).toBe(false);
  });
  test("an IPv4 rule covers the IPv4-mapped IPv6 form", () => {
    expect(hitMapped).toBe(true);
    expect(missMapped).toBe(false);
    expect(missRealV6).toBe(false);
  });
  test("addRange is inclusive on both ends", () => {
    expect(inRange).toBe(true);
    expect(atStart).toBe(true);
    expect(atEnd).toBe(true);
    expect(belowRange).toBe(false);
    expect(aboveRange).toBe(false);
  });
  test("addSubnet matches the CIDR block", () => {
    expect(inSubnet).toBe(true);
    expect(outSubnet).toBe(false);
    expect(hit32).toBe(true);
    expect(miss32).toBe(false);
    expect(hitAny).toBe(true);
  });
  test("IPv6 address and subnet rules", () => {
    expect(hitV6Addr).toBe(true);
    expect(hitV6Subnet).toBe(true);
    expect(missV6Subnet).toBe(false);
  });
  test("a non-byte-aligned IPv6 prefix masks the partial byte", () => {
    expect(hitOdd).toBe(true);
    expect(missOdd).toBe(false);
  });
  test("rules are Node's canonical strings, in insertion order", () => {
    expect(ruleCount).toBe(4);
    expect(rule0).toBe("Address: IPv4 1.2.3.4");
    expect(rule1).toBe("Range: IPv4 10.0.0.1-10.0.0.9");
    expect(rule2).toBe("Subnet: IPv4 172.16.0.0/12");
    expect(rule3).toBe("Address: IPv6 ::1");
  });
  test("toJSON round-trips through fromJSON", () => {
    expect(restoredCount).toBe(4);
    expect(restoredHitAddress).toBe(true);
    expect(restoredHitRange).toBe(true);
    expect(restoredHitSubnet).toBe(true);
    expect(restoredHitV6).toBe(true);
  });
  test("isBlockList tells an instance from anything else", () => {
    expect(isBl).toBe(true);
    expect(isNotBl).toBe(false);
  });
  test("an out-of-range prefix throws ERR_OUT_OF_RANGE", () => {
    expect(badPrefixErr.indexOf("ERR_OUT_OF_RANGE") >= 0).toBe(true);
  });
  test("an invalid address throws ERR_INVALID_ADDRESS", () => {
    expect(badAddrErr.indexOf("ERR_INVALID_ADDRESS") >= 0).toBe(true);
    expect(familyErr.indexOf("ERR_INVALID_ADDRESS") >= 0).toBe(true);
  });
  test("an inverted range is rejected", () => {
    expect(invertedErr.indexOf("ERR_INVALID_ARG_VALUE") >= 0).toBe(true);
  });
  test("check never throws on an unparseable address", () => {
    expect(checkGarbage).toBe(false);
  });
});

describe("node:net SocketAddress", () => {
  test("defaults to 127.0.0.1 / ipv4 / port 0", () => {
    expect(saAddress).toBe("127.0.0.1");
    expect(saFamily).toBe("ipv4");
    expect(saPort).toBe(0);
    expect(saFlow).toBe(0);
  });
  test("family ipv6 defaults the address to ::", () => {
    expect(sa6Address).toBe("::");
    expect(sa6Family).toBe("ipv6");
  });
  test("reads address and port from options", () => {
    expect(saOptsAddress).toBe("192.168.1.9");
    expect(saOptsPort).toBe(8080);
  });
  test("parse reads an IPv4 host:port", () => {
    expect(parsed4Address).toBe("123.1.2.3");
    expect(parsed4Port).toBe(1234);
    expect(parsed4Family).toBe("ipv4");
  });
  test("parse reads a bracketed IPv6 host:port", () => {
    expect(parsed6Address).toBe("1::1");
    expect(parsed6Port).toBe(1234);
    expect(parsed6Family).toBe("ipv6");
  });
  test("parse returns nothing for garbage, without throwing", () => {
    expect(parsedBad === null || parsedBad === undefined).toBe(true);
  });
  test("a SocketAddress is accepted where a string address is", () => {
    expect(hitViaSocketAddress).toBe(true);
  });
});
