// node:buffer — Buffer.isEncoding.
import { describe, test, expect } from "rts:test";
const utf8 = Buffer.isEncoding("utf8");
const hex = Buffer.isEncoding("hex");
const b64 = Buffer.isEncoding("base64");
const bogus = Buffer.isEncoding("bogus-enc");
describe("node:buffer isEncoding", () => {
    test("utf8", () => expect(utf8).toBe(true));
    test("hex", () => expect(hex).toBe(true));
    test("base64", () => expect(b64).toBe(true));
    test("bogus false", () => expect(bogus).toBe(false));
});
