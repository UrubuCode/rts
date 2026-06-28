import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// f64 modulo (libc fmod) — sinal do dividendo, nao do divisor.

// 1. Casos historicos
print(`${5.5 % 2.0}`);     // 1.5
print(`${10.3 % 3.0}`);    // 1.3000000000000007
print(`${-7.5 % 2.0}`);    // -1.5
print(`${1.0 % 1.0}`);     // 0

// 2. Sinais — JS preserva sinal do dividendo
print(`${7.5 % -2.0}`);    // 1.5  (sinal vem de 7.5)
print(`${-7.5 % -2.0}`);   // -1.5

// 3. Dividendo zero
print(`${0.0 % 3.0}`);     // 0
print(`${0.0 % -3.0}`);    // 0

// 4. Divisor zero — IEEE-754 NaN
print(`${5.0 % 0.0}`);     // NaN
print(`${-5.0 % 0.0}`);    // NaN
print(`${0.0 % 0.0}`);     // NaN

// 5. Infinity e NaN — IEEE-754 (igual a JS/Node/Bun): divisor Infinity preserva
// o dividendo; dividendo nao-finito ou NaN propaga NaN.
print(`${Infinity % 3.0}`);   // NaN
print(`${5.0 % Infinity}`);   // 5 (IEEE: x % Infinity === x)
print(`${NaN % 3.0}`);        // NaN
print(`${3.0 % NaN}`);        // NaN

// 6. Resultado em var (path nao-literal)
function fmod(a: number, b: number): number { return a % b; }
print(`${fmod(10.5, 4.0)}`);  // 2.5
print(`${fmod(-3.7, 1.5)}`);  // -0.7000000000000002 (IEEE)

// 7. Em loop — counter de wrap
let total: number = 0;
for (let i = 1; i <= 5; i = i + 1) {
  total = total + (i % 2.0);
}
print(`${total}`);            // 1+0+1+0+1 = 3

describe("f64 modulo — sinais, zero, Inf/NaN, fn, loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "1.5\n1.3000000000000007\n-1.5\n0\n" +
      "1.5\n-1.5\n" +
      "0\n0\n" +
      "NaN\nNaN\nNaN\n" +
      "NaN\n5\nNaN\nNaN\n" +
      "2.5\n-0.7000000000000002\n" +
      "3\n"
    );
  });
});
