import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Generator que nao yielda nada — array vazio.

function* nope(): i64 {
  // sem yields
}

let count: i64 = 0;
for (const _v of nope()) {
  count = count + 1;
}
const h = gc.string_from_i64(count);
print(h); gc.string_free(h);

describe("fixture:generator_empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("0\n");
  });
});
