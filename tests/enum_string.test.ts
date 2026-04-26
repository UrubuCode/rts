import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// String enum basico.

enum Color {
  Red = "vermelho",
  Blue = "azul",
  Green = "verde",
}

print(Color.Red);
print(Color.Blue);
print(Color.Green);

describe("fixture:enum_string", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("vermelho\nazul\nverde\n");
  });
});
