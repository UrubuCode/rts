// node:tls — getCiphers / getCurves (backend data).
import { describe, test, expect } from "rts:test";
import { getCiphers, getCurves } from "node:tls";
const ciphers = getCiphers();
const hasTls13 = ciphers.indexOf("tls_aes_256_gcm_sha384") >= 0;
const ciphersNonEmpty = ciphers.length > 3;
const curves = getCurves();
const hasX25519 = curves.indexOf("x25519") >= 0;
describe("node:tls", () => {
    test("getCiphers has TLS1.3", () => expect(hasTls13).toBe(true));
    test("getCiphers non-empty", () => expect(ciphersNonEmpty).toBe(true));
    test("getCurves has x25519", () => expect(hasX25519).toBe(true));
});
