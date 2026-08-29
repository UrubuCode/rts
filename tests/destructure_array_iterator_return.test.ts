import { describe, test, expect } from "rts:test";

// A pattern that names fewer positions than the source has performs
// IteratorClose, which does nothing while the iterator carries no `return` —
// and `%ArrayIteratorPrototype%` carries none. Adding one to the prototype
// above it makes the close observable, so the indexed path must decline.
// Global and irreversible, hence its own file.

let closed = 0;
const above = Object.getPrototypeOf(Object.getPrototypeOf([].values()));
above.return = function () { closed++; return { done: true }; };

const [head] = [1, 2, 3];

describe("fixture:destructure_array_iterator_return", () => {
  test("the element is still the first one", () => {
    expect(head).toBe(1);
  });

  test("a `return` added above the cursor is called, so the pattern stepped", () => {
    expect(closed).toBe(1);
  });
});
