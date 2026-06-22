import { describe, test, expect } from "rts:test";
import { io, collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Rest sem args passados.

function count(...nums: number[]): number {
    return collections.vec_len(nums);
}

print(`${count()}`); // 0
print(`${count(7, 8, 9, 10, 11)}`); // 5

describe("fixture:rest_param_empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n5\n");
  });
});
