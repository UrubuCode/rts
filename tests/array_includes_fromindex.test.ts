import { describe, test, expect } from "rts:test";

let out = "";

const a = [1, 2, 3, 2, 1];
const r1: boolean = a.includes(2, 2);
const r2: boolean = a.includes(3, 4);
const r3: boolean = a.includes(1, -1);  // negativo
out = (r1?"y":"n") + (r2?"y":"n") + (r3?"y":"n");

describe("array_includes_fromindex", () => {
  test("includes 2-arg (#208)", () => expect(out).toBe("yny"));
});
