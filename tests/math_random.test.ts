import { describe, test, expect } from "rts:test";

let out = "";

// (#208) Math.random() — alias de random_f64.
const r1: number = Math.random();
const r2: number = Math.random();
out += (r1 >= 0 && r1 < 1 ? "in-range" : "out") + "\n";
out += (r2 >= 0 && r2 < 1 ? "in-range" : "out") + "\n";
out += (r1 !== r2 ? "different" : "same") + "\n";  // muito provavel diferente

describe("math_random", () => {
  test("Math.random alias (#208)", () => expect(out).toBe(
    "in-range\nin-range\ndifferent\n"
  ));
});
