import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// for-in em objeto vazio: nenhum corpo executado. (Antes usava o escape hatch
// `collections.map_new()` do motor velho; no motor novo o literal `{}` é a via
// canônica para um objeto vazio.)

const obj = {};
print("before");
for (const key in obj) {
    print("UNREACHABLE");
}
print("after");

describe("fixture:for_in_empty", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("before\nafter\n");
  });
});
