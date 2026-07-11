// node:net — IP address validators.
import { describe, test, expect } from "rts:test";
import { isIP, isIPv4, isIPv6 } from "node:net";

const ip4 = isIP("127.0.0.1");
const ip6 = isIP("::1");
const ipBad = isIP("not-an-ip");
const v4t = isIPv4("192.168.1.1");
const v4f = isIPv4("::1");
const v6t = isIPv6("2001:db8::1");
const v6f = isIPv6("192.168.1.1");

describe("node:net", () => {
    test("isIP v4 → 4", () => expect(ip4).toBe(4));
    test("isIP v6 → 6", () => expect(ip6).toBe(6));
    test("isIP invalid → 0", () => expect(ipBad).toBe(0));
    test("isIPv4 true", () => expect(v4t).toBe(true));
    test("isIPv4 false for v6", () => expect(v4f).toBe(false));
    test("isIPv6 true", () => expect(v6t).toBe(true));
    test("isIPv6 false for v4", () => expect(v6f).toBe(false));
});
