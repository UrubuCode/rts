// node:fs — statfsSync (filesystem statistics, plain object).
import { describe, test, expect } from "rts:test";
import { statfsSync } from "node:fs";

const st = statfsSync(".");
const bsizeOk = st.bsize > 0;
const blocksOk = st.blocks > 0;
const bavailOk = st.bavail >= 0 && st.bavail <= st.blocks;
const isObject = typeof st === "object";

describe("node:fs statfsSync", () => {
    test("returns object", () => expect(isObject).toBe(true));
    test("bsize positive", () => expect(bsizeOk).toBe(true));
    test("blocks positive", () => expect(blocksOk).toBe(true));
    test("bavail within blocks", () => expect(bavailOk).toBe(true));
});
