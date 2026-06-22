import { describe, test, expect } from "rts:test";
import { io, math } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Initializer chamando função user-defined

function compute(): number {
    return 42 + 8;
}

class C {
    a: number = compute(); // 50
    b: number = math.abs_i64(-13); // 13
}

const c = new C();
print(`${c.a}`);

print(`${c.b}`);

describe("fixture:property_init_fn_call", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("50\n13\n");
  });
});
