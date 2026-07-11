// node:crypto — crypto.constants (RSA padding values).
import { describe, test, expect } from "rts:test";
import { constants } from "node:crypto";
const oaep = constants.RSA_PKCS1_OAEP_PADDING;
const pkcs1 = constants.RSA_PKCS1_PADDING;
const nopad = constants.RSA_NO_PADDING;
describe("node:crypto constants", () => {
    test("RSA_PKCS1_OAEP_PADDING", () => expect(oaep).toBe(4));
    test("RSA_PKCS1_PADDING", () => expect(pkcs1).toBe(1));
    test("RSA_NO_PADDING", () => expect(nopad).toBe(3));
});
