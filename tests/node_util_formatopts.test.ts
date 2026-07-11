// node:util — formatWithOptions applies inspect depth to %o.
import { describe, test, expect } from "rts:test";
import { formatWithOptions } from "node:util";

const deep = { a: { b: { c: 1 } } };
const shallowOpts = { depth: 0 };
const s = formatWithOptions(shallowOpts, "obj: %o", deep);
const shallowOk = s.indexOf("[Object]") >= 0 && s.indexOf("obj: ") === 0;

const deepOpts = { depth: 5 };
const d = formatWithOptions(deepOpts, "%o", deep);
const deepOk = d.indexOf("c: 1") >= 0;

describe("node:util formatWithOptions", () => {
    test("depth 0 in %o", () => expect(shallowOk).toBe(true));
    test("depth 5 in %o", () => expect(deepOk).toBe(true));
});
