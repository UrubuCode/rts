import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Arrow com destructuring de array no param.
// Antes: \`forEach(([k,v]) => ...)\` crashava com "undefined variable k"
// no __hoisted_fn_0. Fix: build_fn_decl agora gera prologue
// \`const [k,v] = __hoist_destruct_N_0;\` e renomeia o param para
// __hoist_destruct_N_0.

const pairs: number[][] = [[1, 10], [2, 20], [3, 30]];

const callback = ([k, v]: number[]): number => k + v;
const sum1: number = callback(pairs[0]);
const sum2: number = callback(pairs[1]);
print("p0=" + sum1);
print("p1=" + sum2);

describe("arrow com destructuring de array param", () => {
  test("[k,v] destructure prologue funciona", () =>
    expect(__rtsCapturedOutput).toBe(
      "p0=11\n" +
      "p1=22\n"
    ));
});
