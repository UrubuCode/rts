// node:crypto — fluent chaining now that the engine resolves a builtin-call
// receiver's return-class (createHash(a).update(b).digest(enc)).
import { describe, test, expect } from "rts:test";
import { createHash, createHmac } from "node:crypto";

const sha = createHash("sha256").update("abc").digest("hex");
const md5 = createHash("md5").update("abc").digest("hex");
const hmac = createHmac("sha256", "key").update("The quick brown fox jumps over the lazy dog").digest("hex");

describe("node:crypto chaining", () => {
    test("createHash chain", () => expect(sha).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    test("md5 chain", () => expect(md5).toBe("900150983cd24fb0d6963f7d28e17f72"));
    test("hmac chain", () => expect(hmac).toBe("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"));
});
