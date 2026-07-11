// node:zlib — gzipSync/deflateSync with a level option (round-trip + level effect).
import { describe, test, expect } from "rts:test";
import { gzipSync, gunzipSync, deflateSync, inflateSync } from "node:zlib";

// 64 'a' bytes (char code 97) — compressible.
const data = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

const opt9 = { level: 9 };
const round = gunzipSync(gzipSync(data, opt9));
const roundOk = round.length === 64 && round[0] === 97 && round[63] === 97;

// level 0 (store) is larger than level 9 for compressible data.
const opt0 = { level: 0 };
const g0 = gzipSync(data, opt0);
const g9 = gzipSync(data, opt9);
const levelEffect = g0.length > g9.length;

const dround = inflateSync(deflateSync(data, opt9));
const deflateOk = dround.length === 64 && dround[0] === 97;

describe("node:zlib level", () => {
    test("gzip level round-trip", () => expect(roundOk).toBe(true));
    test("level 0 larger than 9", () => expect(levelEffect).toBe(true));
    test("deflate level round-trip", () => expect(deflateOk).toBe(true));
});
