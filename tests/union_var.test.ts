import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Union em decl de variável: aceita reatribuição com tipos diferentes.
// O valor armazenado mantém os bits — semântica de \"any\" runtime.

function makeNum(): number | string {
    return 42;
}

const v: number | string = makeNum();
print(`${v as number}`);

describe("fixture:union_var", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\n");
  });
});
