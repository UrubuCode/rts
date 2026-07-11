// node:buffer — Buffer.concat with totalLength.
import { describe, test, expect } from "rts:test";
const a = Buffer.from("ab");   // [97,98]
const b = Buffer.from("cd");   // [99,100]
const parts = [a, b];
const full = Buffer.concat(parts);        // 4 bytes
const fullOk = full.length === 4 && full[3] === 100;
const trunc = Buffer.concat(parts, 3);    // truncated to 3
const truncOk = trunc.length === 3 && trunc[0] === 97 && trunc[2] === 99;
const padded = Buffer.concat(parts, 6);   // zero-padded to 6
const padOk = padded.length === 6 && padded[4] === 0 && padded[5] === 0;
describe("node:buffer concat totalLength", () => {
    test("no length", () => expect(fullOk).toBe(true));
    test("truncate", () => expect(truncOk).toBe(true));
    test("zero-pad", () => expect(padOk).toBe(true));
});
