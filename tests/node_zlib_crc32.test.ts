// node:zlib — crc32 (IEEE CRC-32) known vectors.
import { describe, test, expect } from "rts:test";
import { crc32 } from "node:zlib";

const empty = crc32("");           // 0
const abc = crc32("abc");          // 0x352441C2 = 891568578
const chained = crc32("bc", crc32("a")); // incremental == crc32("abc")

describe("node:zlib crc32", () => {
    test("empty", () => expect(empty).toBe(0));
    test("abc", () => expect(abc).toBe(891568578));
    test("incremental chain", () => expect(chained).toBe(891568578));
});
