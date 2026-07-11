// node:buffer — Buffer.byteLength with encoding.
import { describe, test, expect } from "rts:test";
const utf8 = Buffer.byteLength("héllo");        // é=2 bytes → 6
const hex = Buffer.byteLength("616263", "hex"); // → 3
const b64 = Buffer.byteLength("YWJj", "base64"); // → 3
describe("node:buffer byteLength encoding", () => {
    test("utf8", () => expect(utf8).toBe(6));
    test("hex", () => expect(hex).toBe(3));
    test("base64", () => expect(b64).toBe(3));
});
