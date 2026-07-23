import { describe, test, expect } from "rts:test";

// A function's [[Prototype]] is the shared Function.prototype (an object, whose
// own [[Prototype]] is Object.prototype), so getPrototypeOf(fn) is no longer null
// and a function's chain reaches Object.prototype and terminates.

function f(): number { return 1; }
const arrow = (x: number) => x + 1;

const fp = Object.getPrototypeOf(f);            // Function.prototype
const fpNotNull = fp !== null;
const ap = Object.getPrototypeOf(arrow);
const arrowSameFnProto = ap === fp;             // all functions share Function.prototype

const fpProto = Object.getPrototypeOf(fp);      // Object.prototype
const reachesObject = fpProto !== null;
const topIsNull = Object.getPrototypeOf(fpProto) === null; // Object.prototype → null

const fnInstanceofObject = f instanceof Object; // every function IS-A Object
const objProtoOfFn = Object.prototype.isPrototypeOf(f);

describe("function prototype chain", () => {
    test("getPrototypeOf(fn) is not null", () => { expect(fpNotNull).toBe(true); });
    test("all functions share Function.prototype", () => { expect(arrowSameFnProto).toBe(true); });
    test("Function.prototype's proto reaches Object.prototype", () => { expect(reachesObject).toBe(true); });
    test("chain terminates at null above Object.prototype", () => { expect(topIsNull).toBe(true); });
    test("fn instanceof Object", () => { expect(fnInstanceofObject).toBe(true); });
    test("Object.prototype.isPrototypeOf(fn)", () => { expect(objProtoOfFn).toBe(true); });
});
