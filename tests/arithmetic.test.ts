import { describe, test, expect } from "rts:test";
import { io, i32, str } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Basico (caso historico)
const x: i32 = 10;
const y: i32 = 3;
print("sum:" + (x + y));
print("diff:" + (x - y));
print("prod:" + (x * y));
print("quot:" + (x / y));
print("rem:" + (x % y));

// 2. Sinais — negativo, zero
print("neg+pos:" + (-7 + 3));      // -4
print("neg-neg:" + (-7 - -3));     // -4
print("neg*pos:" + (-4 * 5));      // -20
print("neg/pos:" + (-9 / 2));      // -5 (peephole de /2 vira >> 1, sign-extending)
print("zero-x:" + (0 - 12));       // -12
print("x-x:"   + (12 - 12));       // 0

// 3. Precedencia e parenteses
print("p1:" + (1 + 2 * 3));        // 7
print("p2:" + ((1 + 2) * 3));      // 9
print("p3:" + (8 - 4 - 2));        // 2  (left-assoc)
print("p4:" + (8 - (4 - 2)));      // 6
print("p5:" + (2 + 3 * 4 - 1));    // 13
print("p6:" + (20 / 4 / 2));       // 2  (left-assoc)
print("p7:" + (20 / (4 / 2)));     // 10

// 4. Unario
const five: i32 = 5;
print("u1:" + (-five));            // -5
const neg: i32 = -3;
print("u2:" + (-neg));             // 3
const aa: i32 = 7;
print("u3:" + (-aa));              // -7

// 5. Pre/pos increment & decrement preservam ordem em expressao
let c: i32 = 5;
print("post:" + (c++) + ":" + c);  // post:5:6
let d: i32 = 5;
print("pre:" + (++d) + ":" + d);   // pre:6:6
let e: i32 = 5;
print("dec:" + (e--) + ":" + e);   // dec:5:4

// 6. Division integer vs float — RTS faz int div com literais inteiros
const ifour: i32 = 7;
const itwo: i32 = 2;
print("idiv:" + (ifour / itwo));   // 3 (int trunc)
print(`fdiv:${7.0 / 2.0}`);        // 3.5

// 7. Promocao explicita pra f64 via literal com ponto
const half: f64 = 1.0 / 2.0;       // 0.5
const ten: f64 = 10.0;
print(`mix:${half + ten}`);        // 10.5

describe("arithmetic — sinais, precedencia, unario, ++/--", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      // (#584) Atualizado: `/` segue JS spec — sempre f64. quot/neg/pos/p6/idiv
      // refletem divisoes que antes truncavam pra int.
      "sum:13\ndiff:7\nprod:30\nquot:3.3333333333333335\nrem:1\n" +
      "neg+pos:-4\nneg-neg:-4\nneg*pos:-20\nneg/pos:-4.5\nzero-x:-12\nx-x:0\n" +
      "p1:7\np2:9\np3:2\np4:6\np5:13\np6:2.5\np7:10\n" +
      "u1:-5\nu2:3\nu3:-7\n" +
      "post:5:6\npre:6:6\ndec:5:4\n" +
      "idiv:3.5\nfdiv:3.5\n" +
      "mix:10.5\n"
    );
  });
});
