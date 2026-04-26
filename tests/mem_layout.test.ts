import { describe, test, expect } from "rts:test";
import { io, gc, mem } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// mem.size_of_* / align_of_*: layout primitives.

const h1 = gc.string_from_i64(mem.size_of_i64);
print(h1); gc.string_free(h1); // 8
const h2 = gc.string_from_i64(mem.size_of_f64);
print(h2); gc.string_free(h2); // 8
const h3 = gc.string_from_i64(mem.size_of_i32);
print(h3); gc.string_free(h3); // 4
const h4 = gc.string_from_i64(mem.size_of_bool);
print(h4); gc.string_free(h4); // 1
const h5 = gc.string_from_i64(mem.align_of_i64);
print(h5); gc.string_free(h5); // 8

describe("fixture:mem_layout", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("8\n8\n4\n1\n8\n");
  });
});
