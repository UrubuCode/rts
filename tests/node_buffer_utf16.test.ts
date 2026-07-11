// node:buffer — Buffer.from with utf16le encoding.
import { describe, test, expect } from "rts:test";
// "abc" utf16le → [97,0,98,0,99,0]
const b = Buffer.from("abc", "utf16le");
const utf16Ok = b.length === 6 && b[0] === 97 && b[1] === 0 && b[2] === 98 && b[5] === 0;
describe("node:buffer utf16le", () => {
    test("from utf16le", () => expect(utf16Ok).toBe(true));
});
