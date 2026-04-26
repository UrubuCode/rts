import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Intersection: `A & B` aceito como anotação.

interface HasName { name: string; }
interface HasAge { age: number; }

function fixture_describe(p: HasName & HasAge): string {
    return "ok";
}

const obj = { name: "Alice", age: 30 };
print(fixture_describe(obj));

describe("fixture:intersection_type", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("ok\n");
  });
});
