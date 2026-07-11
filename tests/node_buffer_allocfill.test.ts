// node:buffer — Buffer.alloc(size, fill).
import { describe, test, expect } from "rts:test";
const b = Buffer.alloc(4, 97); // four 'a' bytes
const fillOk = b.length === 4 && b[0] === 97 && b[3] === 97;
const z = Buffer.alloc(3); // still zeroed
const zeroOk = z.length === 3 && z[0] === 0;
describe("node:buffer alloc fill", () => {
    test("alloc with fill byte", () => expect(fillOk).toBe(true));
    test("alloc zeroed still works", () => expect(zeroOk).toBe(true));
});
