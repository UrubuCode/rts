// node:buffer — Buffer.alloc throws on negative size.
import { describe, test, expect } from "rts:test";
const okBuf = Buffer.alloc(4).length === 4;
let threw = false;
try { Buffer.alloc(-1); } catch (e) { threw = true; }
describe("node:buffer alloc validation", () => {
    test("valid size", () => expect(okBuf).toBe(true));
    test("negative throws", () => expect(threw).toBe(true));
});
