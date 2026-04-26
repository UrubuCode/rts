import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Intersection eh type-only (TS structural). Runtime usa o object literal.

interface HasName { name: string; }
interface HasAge { age: i64; }

// Apenas valida que TS aceita a anotacao em type alias / declaration site.
const obj: HasName & HasAge = { name: "Mario", age: 35 };
const ageStr = gc.string_from_i64(obj.age);
print(obj.name + " com " + ageStr);
gc.string_free(ageStr);

describe("fixture:intersection_extended", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("Mario com 35\n");
  });
});
