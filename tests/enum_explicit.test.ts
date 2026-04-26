import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Enum com valores explícitos e mistos (auto-incremento após explicit).

enum Mask {
    Read = 1,
    Write = 2,
    Exec = 4,
    All = 7, // explicit
    Sentinel,  // 8 (All + 1)
}

const h1 = gc.string_from_i64(Mask.Read);
print(h1); gc.string_free(h1); // 1
const h2 = gc.string_from_i64(Mask.Write);
print(h2); gc.string_free(h2); // 2
const h3 = gc.string_from_i64(Mask.All);
print(h3); gc.string_free(h3); // 7
const h4 = gc.string_from_i64(Mask.Sentinel);
print(h4); gc.string_free(h4); // 8

// Bitmask: Read | Write
const rw = Mask.Read | Mask.Write;
const h5 = gc.string_from_i64(rw);
print(h5); gc.string_free(h5); // 3

describe("fixture:enum_explicit", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n7\n8\n3\n");
  });
});
