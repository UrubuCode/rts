import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Namespace básico com fns exportadas.

namespace Math2 {
    // Limitação MVP: fns no namespace devem retornar tipos com lane i64
    // (i64/i32/handle/bool). `number` mapeia pra F64 e o call_indirect
    // genérico só sabe i64 — follow-up exige sig por entry no map.
    export function double(x: i64): i64 {
        return x * 2;
    }
    export function triple(x: i64): i64 {
        return x * 3;
    }
}

const h1 = gc.string_from_i64(Math2.double(5));
print(h1); gc.string_free(h1); // 10

const h2 = gc.string_from_i64(Math2.triple(7));
print(h2); gc.string_free(h2); // 21

describe("fixture:namespace_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("10\n21\n");
  });
});
