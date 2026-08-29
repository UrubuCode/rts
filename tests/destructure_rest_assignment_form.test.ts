import { describe, test, expect } from "rts:test";

// A rest element must behave the same whether the pattern DECLARES names or
// ASSIGNS to places. The two reach the emitter through different parser paths,
// and only the declaring one used to push a placeholder for the rest's own
// slot — so the emitter, which drops the last element when a pattern has a
// rest, dropped a real target in the assigning form instead of the placeholder.

let c: any, d: any;
[c, ...d] = [1, 2, 3];

const [e, ...f] = [1, 2, 3];

let g: any, h: any;
[, g, ...h] = [1, 2, 3, 4];

let i: any;
[...i] = [1, 2];

describe("fixture:destructure_rest_assignment_form", () => {
  test("the assigning form keeps the element before the rest", () => {
    expect(c).toBe(1);
    expect(JSON.stringify(d)).toBe("[2,3]");
  });

  test("the declaring form is unchanged", () => {
    expect(e).toBe(1);
    expect(JSON.stringify(f)).toBe("[2,3]");
  });

  test("a hole before the named element still counts as a position", () => {
    expect(g).toBe(2);
    expect(JSON.stringify(h)).toBe("[3,4]");
  });

  test("a rest with nothing before it takes everything", () => {
    expect(JSON.stringify(i)).toBe("[1,2]");
  });
});
