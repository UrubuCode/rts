import { describe, test, expect } from "rts:test";

// Replacing `%ArrayIteratorPrototype%.next` is global and cannot be undone, so
// it gets a file to itself. This is the sharpest test of the guard that lets an
// array pattern read by index: a pattern that skipped the protocol here would
// answer [1,2] and never call the replacement.

const before = (() => { const [a, b] = [1, 2]; return JSON.stringify([a, b]); })();

const cursor = Object.getPrototypeOf([].values());
cursor.next = function () { return { value: 99, done: false }; };

const after = (() => { const [a, b] = [1, 2]; return JSON.stringify([a, b]); })();

describe("fixture:destructure_array_patched_protocol", () => {
  test("the pattern reads directly while the step is the primordial one", () => {
    expect(before).toBe("[1,2]");
  });

  test("a replaced `next` is observed, so the pattern must step", () => {
    expect(after).toBe("[99,99]");
  });
});
