// node:util — inspect deep renderer + %o formatting.
import { describe, test, expect } from "rts:test";
import { inspect, format } from "node:util";

const iNum = inspect(42);
const iStr = inspect("hi");
const iBool = inspect(true);
const iNull = inspect(null);
const iArr = inspect([1, 2, 3]);
const iEmptyArr = inspect([]);
const iObj = inspect({ a: 1, b: "two" });
const iNested = inspect({ x: [1, 2], y: { z: 3 } });
const iEmptyObj = inspect({});

const fmtO = format("val: %o", [1, 2]);

describe("node:util inspect", () => {
    test("number", () => expect(iNum).toBe("42"));
    test("string quoted", () => expect(iStr).toBe("'hi'"));
    test("bool", () => expect(iBool).toBe("true"));
    test("null", () => expect(iNull).toBe("null"));
    test("array", () => expect(iArr).toBe("[ 1, 2, 3 ]"));
    test("empty array", () => expect(iEmptyArr).toBe("[]"));
    test("object", () => expect(iObj).toBe("{ a: 1, b: 'two' }"));
    test("nested", () => expect(iNested).toBe("{ x: [ 1, 2 ], y: { z: 3 } }"));
    test("empty object", () => expect(iEmptyObj).toBe("{}"));
    test("format %o", () => expect(fmtO).toBe("val: [ 1, 2 ]"));
});
