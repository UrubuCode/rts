import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Union em parâmetro: aceita ambos os tipos.

function fixture_describe(x: string | number): string {
    return "got";
}

print(fixture_describe(42));
print(fixture_describe("hello"));
print(fixture_describe(0));

describe("fixture:union_param", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("got\ngot\ngot\n");
  });
});
