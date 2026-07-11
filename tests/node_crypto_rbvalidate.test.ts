// node:crypto — randomBytes validates size.
import { describe, test, expect } from "rts:test";
import { randomBytes } from "node:crypto";
const ok = randomBytes(8).length === 8;
let threw = false;
try { randomBytes(-1); } catch (e) { threw = true; }
describe("node:crypto randomBytes validation", () => {
    test("valid size", () => expect(ok).toBe(true));
    test("negative size throws", () => expect(threw).toBe(true));
});
