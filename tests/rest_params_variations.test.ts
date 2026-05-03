import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Caso 1: rest com 0 args extras.
function tail0(a: number, ...rest: number[]): number {
  return rest.length;
}
const r0 = tail0(42);

// Caso 2: rest com acesso a [0].
function firstRest(...rest: number[]): number {
  return rest[0];
}
const r1 = firstRest(7, 8, 9);

// Caso 3: rest soma elementos via for...of.
function sumRest(...rest: number[]): number {
  let s = 0;
  for (const n of rest) s += n;
  return s;
}
const r2 = sumRest(1, 2, 3, 4);

// Caso 4: rest combinado com spread no caller (literal).
function add3(a: number, b: number, c: number): number {
  return a + b + c;
}
const r3 = add3(...[10, 20, 30]);

// Caso 5: spread de variavel const.
const xs = [100, 200, 300];
const r4 = add3(...xs);

print(`${r0}`);
print(`${r1}`);
print(`${r2}`);
print(`${r3}`);
print(`${r4}`);

describe("rest_params_variations", () => {
  test("rest=0 quando caller nao passa extras", () => expect(r0).toBe(0));
  test("rest[0] retorna primeiro extra", () => expect(r1).toBe(7));
  test("for...of em rest soma corretamente", () => expect(r2).toBe(10));
  test("spread literal no caller", () => expect(r3).toBe(60));
  test("spread de const no caller", () => expect(r4).toBe(600));
  test("output capturado", () =>
    expect(__rtsCapturedOutput).toBe("0\n7\n10\n60\n600\n"));
});
