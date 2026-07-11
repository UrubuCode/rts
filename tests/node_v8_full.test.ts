// node:v8 — structured-clone serialize/deserialize round-trip (RTS wire format).
import { describe, test, expect } from "rts:test";
import { serialize, deserialize } from "node:v8";

// primitives
const n = deserialize(serialize(42));
const nOk = n === 42;
const f = deserialize(serialize(3.5));
const fOk = f === 3.5;
const s = deserialize(serialize("hello"));
const sOk = s === "hello";
const b = deserialize(serialize(true));
const bOk = b === true;
const nul = deserialize(serialize(null));
const nulOk = nul === null;

// array
const arr = deserialize(serialize([1, 2, 3]));
const arrOk = arr.length === 3 && arr[0] === 1 && arr[2] === 3;

// nested array
const nested = deserialize(serialize([1, [2, 3], 4]));
const nestedOk = nested.length === 3 && nested[1].length === 2 && nested[1][1] === 3;

// object
const obj = deserialize(serialize({ a: 1, b: "two" }));
const objOk = obj.a === 1 && obj.b === "two";

// nested object + mixed
const deep = deserialize(serialize({ name: "x", nums: [10, 20], flag: false }));
const deepOk = deep.name === "x" && deep.nums.length === 2 && deep.nums[1] === 20 && deep.flag === false;

// serialize returns a byte buffer
const buf = serialize(1);
const bufOk = buf.length > 0;

describe("node:v8", () => {
    test("number round-trip", () => expect(nOk).toBe(true));
    test("float round-trip", () => expect(fOk).toBe(true));
    test("string round-trip", () => expect(sOk).toBe(true));
    test("bool round-trip", () => expect(bOk).toBe(true));
    test("null round-trip", () => expect(nulOk).toBe(true));
    test("array round-trip", () => expect(arrOk).toBe(true));
    test("nested array", () => expect(nestedOk).toBe(true));
    test("object round-trip", () => expect(objOk).toBe(true));
    test("deep mixed object", () => expect(deepOk).toBe(true));
    test("serialize returns bytes", () => expect(bufOk).toBe(true));
});
