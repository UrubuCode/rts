import { describe, test, expect } from "rts:test";
import { io, gc, math } from "rts";

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
const ha = gc.string_from_i64(c.a);
print(ha); // 5
gc.string_free(ha);

const hb = gc.string_from_i64(c.b);
print(hb); // 7
gc.string_free(hb);

print(c.s); // hello world

describe("fixture:property_init_expr", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("5\n7\nhello world\n");
  });
});
