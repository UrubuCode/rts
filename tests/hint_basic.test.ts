import { describe, test, expect } from "rts:test";
import { io, gc, hint } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// hint.* — primitivos de otimizacao.

// black_box impede otimizacao da expressao constante.
const a = hint.black_box_i64(42);
const h1 = gc.string_from_i64(a); print(h1); gc.string_free(h1);

// spin_loop nao tem efeito visivel — apenas roda sem panic.
hint.spin_loop();
print("spin-ok");

// assert_unchecked com cond=true (debug e release ok).
hint.assert_unchecked(true);
print("assert-ok");

describe("fixture:hint_basic", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("42\nspin-ok\nassert-ok\n");
  });
});
