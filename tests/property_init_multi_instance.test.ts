import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Cada instância recebe sua própria cópia dos initializers

class C {
    n: number = 100;
}

const a = new C();
const b = new C();
const k = new C();

a.n = 1;
b.n = 2;
// k.n permanece 100 (initializer)

const ha = gc.string_from_i64(a.n);
const hb = gc.string_from_i64(b.n);
const hk = gc.string_from_i64(k.n);
print(ha); gc.string_free(ha); // 1
print(hb); gc.string_free(hb); // 2
print(hk); gc.string_free(hk); // 100

describe("fixture:property_init_multi_instance", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("1\n2\n100\n");
  });
});
