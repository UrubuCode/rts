import { describe, test, expect } from "rts:test";
import { io, mem } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// mem.size_of_* / align_of_*: layout primitives.

print(`${mem.size_of_i64}`); // 8
print(`${mem.size_of_f64}`); // 8
print(`${mem.size_of_i32}`); // 4
print(`${mem.size_of_bool}`); // 1
print(`${mem.align_of_i64}`); // 8

describe("fixture:mem_layout", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("8\n8\n4\n1\n8\n");
  });
});
