import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Casos historicos
print(`${2 ** 10}`);       // 1024
print(`${2.0 ** 0.5}`);    // 1.4142135623730951
print(`${3 ** 3}`);        // 27
print(`${10 ** -2}`);      // 0.01

// 2. Identidades
print(`${5 ** 0}`);        // 1
print(`${0 ** 0}`);        // 1  (JS: 0**0 = 1)
print(`${1 ** 100}`);      // 1
print(`${0 ** 5}`);        // 0
print(`${7 ** 1}`);        // 7

// 3. Base negativa
print(`${(-2) ** 3}`);     // -8
print(`${(-2) ** 4}`);     // 16
print(`${(-3) ** 0}`);     // 1

// 4. Expoente negativo (fracao)
print(`${2 ** -3}`);       // 0.125
print(`${4 ** -0.5}`);     // 0.5

// 5. Right associative — `a ** b ** c == a ** (b ** c)`
print(`${2 ** 3 ** 2}`);   // 2 ** 9 = 512  (NAO 8**2=64)
print(`${(2 ** 3) ** 2}`); // 64

// 6. Em fn user
function pow2(n: number): number { return 2 ** n; }
print(`${pow2(5)}`);       // 32
print(`${pow2(0)}`);       // 1
print(`${pow2(-1)}`);      // 0.5

// 7. Em loop — somatorio de potencias de 2
let s: number = 0;
for (let i = 0; i < 5; i = i + 1) {
  s = s + (2 ** i);
}
print(`${s}`);             // 1+2+4+8+16 = 31

describe("exponentiation — identidades, neg base/exp, right-assoc, fn, loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "1024\n1.4142135623730951\n27\n0.01\n" +
      "1\n1\n1\n0\n7\n" +
      "-8\n16\n1\n" +
      "0.125\n0.5\n" +
      "512\n64\n" +
      "32\n1\n0.5\n" +
      "31\n"
    );
  });
});
