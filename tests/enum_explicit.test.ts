import { describe, test, expect } from "rts:test";
import { io } from "rts";

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

print(`${Mask.Read}`); // 1
print(`${Mask.Write}`); // 2
print(`${Mask.All}`); // 7
print(`${Mask.Sentinel}`); // 8

// Bitmask: Read | Write
const rw = Mask.Read | Mask.Write;
print(`${rw}`); // 3

describe("fixture:enum_explicit", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n7\n8\n3\n");
  });
});
