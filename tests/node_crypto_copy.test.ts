// node:crypto — hash.copy() clones the state for independent digests.
import { describe, test, expect } from "rts:test";
import { createHash } from "node:crypto";
const h = createHash("sha256");
h.update("ab");
const c = h.copy();
// continue both independently.
h.update("c");   // h = sha256("abc")
c.update("d");   // c = sha256("abd")
const hHex = h.digest("hex");
const cHex = c.digest("hex");
const abcOk = hHex === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const differ = cHex !== hHex && cHex.length === 64;
describe("node:crypto hash.copy", () => {
    test("original continues (abc)", () => expect(abcOk).toBe(true));
    test("copy diverges (abd)", () => expect(differ).toBe(true));
});
