import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Sub com ctor explícito mas SEM super() chamado:
// initializers ficam no inicio (sem super pra pular).
// Limitação documentada da feature: parent não inicializa,
// mas é semântica válida quando parent é trivial.

class Base {
    a: number = 7;
}

class Sub extends Base {
    b: number = 13;
    constructor() {
        // Não chamamos super() aqui — initializer de Sub.b roda mesmo assim.
        // a fica em estado "indefinido" (no caso, 0 porque o map está vazio).
    }
}

const s = new Sub();
const hb = gc.string_from_i64(s.b);
print(hb); gc.string_free(hb); // 13

describe("fixture:property_init_no_super_call", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("13\n");
  });
});
