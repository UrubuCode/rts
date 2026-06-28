import { describe, test, expect } from "rts:test";
import { io, num } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// num.checked_*: aritmetica que sinaliza overflow via i64::MIN.
// NOTA: o motor novo nao tem repr Int64 (so Int32/Float64, como JS). Valores
// no limite i64 (i64::MIN) viram f64 e imprimem como JS/Node imprimiriam
// (-9223372036854776000). Era exato no motor velho (i64 sobrecarregado, deletado).

const a = num.checked_add(100, 200);
print(`${a}`);

const b = num.checked_div(100, 0);
print(`${b}`);

const c = num.checked_sub(50, 30);
print(`${c}`);

const d = num.checked_mul(7, 6);
print(`${d}`);

describe("fixture:num_checked", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe("300\n-9223372036854776000\n20\n42\n");
  });
});
