// node:util — inspect with a depth option.
import { describe, test, expect } from "rts:test";
import { inspect } from "node:util";

// deeply nested object; default depth-2 collapses the 3rd level to [Object].
const deep = { a: { b: { c: { d: 1 } } } };

const optShallow = { depth: 0 };
const shallow = inspect(deep, optShallow);
// depth 0 → the first nested object is collapsed.
const shallowOk = shallow.indexOf("[Object]") >= 0;

const optDeep = { depth: 5 };
const full = inspect(deep, optDeep);
// depth 5 → the innermost d: 1 is rendered.
const fullOk = full.indexOf("d: 1") >= 0 && full.indexOf("[Object]") < 0;

describe("node:util inspect depth", () => {
    test("depth 0 collapses", () => expect(shallowOk).toBe(true));
    test("depth 5 expands fully", () => expect(fullOk).toBe(true));
});
