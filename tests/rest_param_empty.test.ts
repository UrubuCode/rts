import { describe, test, expect } from "rts:test";
import { io, gc, collections } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Rest sem args passados.

function count(...nums: number[]): number {
    return collections.vec_len(nums);
}

const h1 = gc.string_from_i64(count());
print(h1); gc.string_free(h1); // 0
const h2 = gc.string_from_i64(count(7, 8, 9, 10, 11));
print(h2); gc.string_free(h2); // 5

describe("fixture:rest_param_empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n5\n");
  });
});
