// node:util — format %j does JSON.stringify.
import { describe, test, expect } from "rts:test";
import { format } from "node:util";
const obj = { a: 1, b: "two", c: [1, 2] };
const s = format("%j", obj);
const objOk = s === '{"a":1,"b":"two","c":[1,2]}';
const arr = format("%j", [1, 2, 3]);
const arrOk = arr === "[1,2,3]";
const str = format("%j", "he\"llo");
const strOk = str === '"he\\"llo"';
describe("node:util format %j", () => {
    test("object json", () => expect(objOk).toBe(true));
    test("array json", () => expect(arrOk).toBe(true));
    test("string escaped", () => expect(strOk).toBe(true));
});
