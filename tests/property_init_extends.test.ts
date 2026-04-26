import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Subclasse: initializers rodam DEPOIS de super(), antes do user code

class Base {
    a: number = 10;
    constructor() {
        // Base initializer: a=10
    }
}

class Sub extends Base {
    b: number = 20;
    c: number;

    constructor() {
        super();
        // após super: a=10. depois rolam initializers de Sub: b=20.
        // Aí o user code aqui:
        this.c = this.a + this.b; // 30
    }
}

const s = new Sub();
const ha = gc.string_from_i64(s.a);
print(ha); // 10
gc.string_free(ha);

const hb = gc.string_from_i64(s.b);
print(hb); // 20
gc.string_free(hb);

const hc = gc.string_from_i64(s.c);
print(hc); // 30
gc.string_free(hc);

describe("fixture:property_init_extends", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n20\n30\n");
  });
});
