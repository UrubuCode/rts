// util.promisify — the callback-to-promise wrapper, and the three ways it lies
// if it is written carelessly.
//
// The interesting assertions are not "it returns a promise". They are: that the
// wrapper REMEMBERS which function it wraps (a native cannot close over
// anything, so this is the part that needs a mechanism), that a truthy first
// callback argument REJECTS rather than fulfilling with an error object, and
// that a fourth argument does not fall off the end — the calling convention
// here is four slots, and the callback has to go somewhere.
import { describe, test, expect } from "rts:test";
import { promisify } from "node:util";

function doubleLater(value: number, callback: Function) {
    setTimeout(() => callback(null, value * 2), 5);
}

function failLater(callback: Function) {
    setTimeout(() => callback(new Error("boom")), 5);
}

// Four real arguments plus the callback: past the four-slot convention, so a
// wrapper that reads fixed slots drops one or has nowhere to put the callback.
function sumFour(a: number, b: number, c: number, d: number, callback: Function) {
    callback(null, a + b + c + d);
}

const doubled = await promisify(doubleLater)(21);
const summed = await promisify(sumFour)(1, 2, 3, 4);

let rejected = "not rejected";
try {
    await promisify(failLater)();
} catch (error) {
    rejected = error.message;
}

// Node's own escape hatch: a function may declare its own promise form.
function custom(value: number, callback: Function) {
    callback(null, "the callback form");
}
custom[Symbol.for("nodejs.util.promisify.custom")] = (value: number) =>
    Promise.resolve("the custom form");
const chosen = await promisify(custom)(1);

describe("util.promisify", () => {
    test("it fulfils with the callback's value", () => expect(doubled).toBe(42));
    test("a truthy error rejects", () => expect(rejected).toBe("boom"));
    test("more arguments than slots survive", () => expect(summed).toBe(10));
    test("the custom hook wins over the callback", () => expect(chosen).toBe("the custom form"));
    // Not `promisify(f) === promisify(f)` — each call mints a wrapper, and Node
    // does the same. What is idempotent is promisifying an ALREADY-promisified
    // function: the wrapper declares itself its own custom form.
    test("promisifying a wrapper answers the wrapper", () => {
        const wrapped = promisify(doubleLater);
        expect(promisify(wrapped)).toBe(wrapped);
    });
});
