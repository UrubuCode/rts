import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Union de literais numericos.

function rate(stars: 1 | 2 | 3 | 4 | 5): string {
  if (stars <= 2) return "ruim";
  if (stars == 3) return "medio";
  return "bom";
}

print(rate(1));
print(rate(3));
print(rate(5));

describe("fixture:union_number_literal", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("ruim\nmedio\nbom\n");
  });
});
