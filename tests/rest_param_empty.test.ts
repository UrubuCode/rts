import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Rest sem args passados. Lê o tamanho via `.length` (via canônica JS; antes
// usava o escape hatch `collections.vec_len(nums)` do motor velho onde arrays
// eram Vec handles — no motor novo rest params são shape-arrays).

function count(...nums: number[]): number {
    return nums.length;
}

print(`${count()}`); // 0
print(`${count(7, 8, 9, 10, 11)}`); // 5

describe("fixture:rest_param_empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n5\n");
  });
});
