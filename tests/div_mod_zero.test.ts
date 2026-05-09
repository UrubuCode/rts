import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#296) RTS antes crashava com 'Illegal instruction' em x/0 e x%0 com
// inteiros. (#584) Hoje `/` segue o JS spec — sempre retorna f64, entao
// 5/0 = Infinity (IEEE) tanto pra literal int quanto float. So' o `%`
// inteiro ainda retorna sentinel 0 em divisor 0 (RTS-specific, evita trap).

// 1. Casos do issue — `/` IEEE em qualquer caso, `%` int sentinel, `%` float NaN
print(`${5 / 0}`);          // Infinity (JS spec — `/` sempre f64)
print(`${5.0 / 0.0}`);      // Infinity
print(`${-5.0 / 0.0}`);     // -Infinity
print(`${0.0 / 0.0}`);      // NaN
print(`${5 % 0}`);          // 0    (int sentinel — evita trap)
print(`${5.0 % 0.0}`);      // NaN

// 2. Caminho normal preservado (sem trap penalty)
print(`${10 / 2}`);   // 5  (5.0 mas formata como "5")
print(`${10 % 3}`);   // 1
print(`${-7 % 3}`);   // -1 (sinal preservado, fix #297)

// 3. Combinado com guard ternario — evita o Infinity
const a: number = 0;
print(`${a !== 0 ? 100 / a : "guarded"}`);  // guarded
print(`${a === 0 ? "skip" : 100 / a}`);     // skip

// 4. Em loop — counter sempre nao-zero. Como `/` agora eh f64,
// 10/1+10/2+10/3+10/4+10/5 = 10+5+3.333...+2.5+2 = 22.833...
let total: number = 0;
for (let i = 1; i <= 5; i++) {
  total = total + (10 / i);
}
print(`${total}`);  // 22.833333333333332

// 5. Fn user com div em path sem guard explicito (number = f64, IEEE)
function divUnsafe(a: number, b: number): number {
  return a / b;
}
print(`${divUnsafe(10, 2)}`);  // 5
print(`${divUnsafe(10, 0)}`);  // Infinity (f64 IEEE)

// 6. Classe com static method usando div
class Calculator {
  static safeDiv(a: number, b: number): number {
    if (b === 0) return -1;
    return a / b;
  }
}
print(`${Calculator.safeDiv(20, 4)}`);  // 5
print(`${Calculator.safeDiv(20, 0)}`);  // -1

// 7. Try/catch — div/0 nao throw em JS, nem em RTS agora
try {
  const x = 1 / 0;
  print(`tried: ${x}`);    // tried: Infinity (JS spec)
} catch (e) {
  print("nao deveria throw");
}

// 8. Modulo em counter de loop — caso comum
let evens = 0;
for (let i = 1; i <= 10; i++) {
  if (i % 2 === 0) evens = evens + 1;
}
print(`${evens}`);  // 5

// 9. Mod por zero em runtime variable (number = f64)
function modBy(n: number, k: number): number {
  return n % k;
}
print(`${modBy(7, 3)}`);  // 1
print(`${modBy(7, 0)}`);  // NaN (f64 IEEE)

describe("div_mod_zero", () => {
  test("no trap, sentinel 0 in int /0, IEEE in float /0", () =>
    expect(__rtsCapturedOutput).toBe(
      "Infinity\nInfinity\n-Infinity\nNaN\n0\nNaN\n" +  // 1
      "5\n1\n-1\n" +                                    // 2
      "guarded\nskip\n" +                               // 3
      "22.833333333333332\n" +                          // 4
      "5\nInfinity\n" +                                 // 5
      "5\n-1\n" +                                       // 6
      "tried: Infinity\n" +                             // 7
      "5\n" +                                           // 8
      "1\nNaN\n"                                        // 9
    ));
});
