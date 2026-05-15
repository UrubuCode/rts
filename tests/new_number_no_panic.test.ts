import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// `new Number(42)` antes panicava com Cranelift type mismatch
// porque o codegen assumia que constructor retornava Handle (i64)
// mas Number_NEW_FROM retorna f64. Agora usamos ValTy::from_abi(returns)
// pra preservar o tipo correto.
const n = new Number(42);
print("n=" + n);

const z = new Number(0);
print("z=" + z);

// Verifica que tambem nao quebra outros usos comuns.
const arr = [new Number(1), new Number(2), new Number(3)];
print("arr_len=" + arr.length);

describe("new Number(x) preserves Cranelift type (panic fix)", () => {
  test("matches expected stdout", () =>
    expect(out).toBe(
      "n=42\n" +
      "z=0\n" +
      "arr_len=3\n"
    )
  );
});
