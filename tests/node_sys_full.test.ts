// node:sys — the deprecated alias of node:util.
import { describe, test, expect } from "rts:test";
import { format, isDeepStrictEqual } from "node:sys";
const f = format("%s = %d", "n", 42);
const de = isDeepStrictEqual([1, 2], [1, 2]);
describe("node:sys", () => {
    test("format (via sys)", () => expect(f).toBe("n = 42"));
    test("isDeepStrictEqual (via sys)", () => expect(de).toBe(true));
});
