import { describe, test, expect } from "rts:test";
import { io, math } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Initializer com expressão composta (não só literal)

class C {
    a: number = 2 + 3;
    b: number = math.abs_i64(-7);
    s: string = "hello" + " world";
}

const c = new C();
print(`${c.a}`); // 5

print(`${c.b}`); // 7

print(c.s); // hello world

describe("fixture:property_init_expr", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n7\nhello world\n");
  });
});
