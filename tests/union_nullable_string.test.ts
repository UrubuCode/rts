import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// `string | null`: Handle nullable. Branch null → fallback.

function greet(name: string | null): string {
  if (name == null) return "ola desconhecido";
  return "ola " + name;
}

print(greet("Mario"));
print(greet(null));
print(greet("Ana"));

describe("fixture:union_nullable_string", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("ola Mario\nola desconhecido\nola Ana\n");
  });
});
