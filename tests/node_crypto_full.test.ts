// node:crypto — hashing / HMAC / random. Known-answer vectors.
// NOTE: the Hash object's class is tracked through a `const h = createHash(...)`
// binding (not through an inline `createHash(...).update(...)` chain — the engine
// does not yet propagate a call-expression's return-class through a chained
// method call), so each instance is bound before use.
import { describe, test, expect } from "rts:test";
import {
    createHash,
    createHmac,
    hash,
    randomBytes,
    randomUUID,
    randomInt,
    timingSafeEqual,
    getHashes,
} from "node:crypto";

// SHA-256("abc") — canonical NIST vector.
const h256 = createHash("sha256");
h256.update("abc");
const sha256abc = h256.digest("hex");
const sha256Ok = sha256abc === "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

// MD5("abc") = 900150983cd24fb0d6963f7d28e17f72
const hmd5 = createHash("md5");
hmd5.update("abc");
const md5Ok = hmd5.digest("hex") === "900150983cd24fb0d6963f7d28e17f72";

// SHA-1("abc") = a9993e364706816aba3e25717850c26c9cd0d89d
const hsha1 = createHash("sha1");
hsha1.update("abc");
const sha1Ok = hsha1.digest("hex") === "a9993e364706816aba3e25717850c26c9cd0d89d";

// One-shot crypto.hash — same result.
const oneShotOk = hash("sha256", "abc") === sha256abc;

// base64 digest of sha256("abc").
const hb64 = createHash("sha256");
hb64.update("abc");
const b64Ok = hb64.digest("base64") === "ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0=";

// HMAC-SHA256(key="key", "The quick brown fox jumps over the lazy dog")
const hm = createHmac("sha256", "key");
hm.update("The quick brown fox jumps over the lazy dog");
const hmacOk = hm.digest("hex") === "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8";

// incremental update (two chunks) equals single update.
const hinc = createHash("sha256");
hinc.update("a");
hinc.update("bc");
const incOk = hinc.digest("hex") === sha256abc;

// randomBytes length.
const rbOk = randomBytes(16).length === 16;

// randomUUID v4 shape.
const uuid = randomUUID();
const uuidOk = uuid.length === 36 && uuid.charAt(14) === "4" && uuid.charAt(8) === "-";

// randomInt.
const riOk = randomInt(1, 2) === 1;
const ri2 = randomInt(10);
const ri2Ok = ri2 >= 0 && ri2 < 10;

// timingSafeEqual.
const tseEq = timingSafeEqual("hello", "hello");
const tseNe = timingSafeEqual("hello", "world");

// getHashes.
const hashes = getHashes();
const hashesOk = hashes.indexOf("sha256") >= 0 && hashes.indexOf("md5") >= 0;

describe("node:crypto", () => {
    test("sha256(abc)", () => expect(sha256Ok).toBe(true));
    test("md5(abc)", () => expect(md5Ok).toBe(true));
    test("sha1(abc)", () => expect(sha1Ok).toBe(true));
    test("crypto.hash oneshot", () => expect(oneShotOk).toBe(true));
    test("digest base64", () => expect(b64Ok).toBe(true));
    test("hmac-sha256", () => expect(hmacOk).toBe(true));
    test("incremental update", () => expect(incOk).toBe(true));
    test("randomBytes length", () => expect(rbOk).toBe(true));
    test("randomUUID v4", () => expect(uuidOk).toBe(true));
    test("randomInt(1,2)", () => expect(riOk).toBe(true));
    test("randomInt(10)", () => expect(ri2Ok).toBe(true));
    test("timingSafeEqual equal", () => expect(tseEq).toBe(true));
    test("timingSafeEqual unequal", () => expect(tseNe).toBe(false));
    test("getHashes", () => expect(hashesOk).toBe(true));
});
