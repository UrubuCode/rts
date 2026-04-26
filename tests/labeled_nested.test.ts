import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 3 níveis: break com label sobe direto pra um intermediário.

let count: number = 0;
outer: for (let i = 0; i < 3; i = i + 1) {
    middle: for (let j = 0; j < 3; j = j + 1) {
        for (let k = 0; k < 3; k = k + 1) {
            if (k == 1) {
                break middle;
            }
            count = count + 1;
        }
        // não executa por causa do break middle
        count = count + 100;
    }
}

const h = gc.string_from_i64(count);
print(h); gc.string_free(h); // 3 iter externas × 1 inc = 3

describe("fixture:labeled_nested", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("3\n");
  });
});
