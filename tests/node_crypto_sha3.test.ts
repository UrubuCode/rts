// node:crypto — SHA-3 support (NIST vectors).
import { describe, test, expect } from "rts:test";
import { createHash, getHashes } from "node:crypto";

// SHA3-256("abc") = 3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532
const h256 = createHash("sha3-256");
h256.update("abc");
const sha3_256Ok = h256.digest("hex") === "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532";

// SHA3-512("abc") = b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0
const h512 = createHash("sha3-512");
h512.update("abc");
const sha3_512Ok = h512.digest("hex") === "b751850b1a57168a5693cd924b6b096e08f621827444f70d884f5d0240d2712e10e116e9192af3c91a7ec57647e3934057340b4cf408d5a56592f8274eec53f0";

const hashes = getHashes();
const listedOk = hashes.indexOf("sha3-256") >= 0 && hashes.indexOf("sha3-512") >= 0;

describe("node:crypto sha3", () => {
    test("sha3-256(abc)", () => expect(sha3_256Ok).toBe(true));
    test("sha3-512(abc)", () => expect(sha3_512Ok).toBe(true));
    test("getHashes lists sha3", () => expect(listedOk).toBe(true));
});
