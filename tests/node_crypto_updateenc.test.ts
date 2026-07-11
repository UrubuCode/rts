// node:crypto — hash.update(data, inputEncoding).
import { describe, test, expect } from "rts:test";
import { createHash } from "node:crypto";
// SHA-256 of the bytes 0x61 0x62 0x63 ("abc") given as hex "616263" == SHA-256("abc").
const h = createHash("sha256");
h.update("616263", "hex");
const hexInputOk = h.digest("hex") === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
// base64 input.
const h2 = createHash("sha256");
h2.update("YWJj", "base64"); // "abc"
const b64InputOk = h2.digest("hex") === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
describe("node:crypto update encoding", () => {
    test("hex input", () => expect(hexInputOk).toBe(true));
    test("base64 input", () => expect(b64InputOk).toBe(true));
});
