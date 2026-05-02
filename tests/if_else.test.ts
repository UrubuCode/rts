import { describe, test, expect } from "rts:test";
import { io, i32 } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Caso historico
let x: i32 = 5;
if (x > 3) print("big"); else print("small");
if (x === 5) print("five");

// 2. else if chain — primeiro match vence
function bucket(n: i32): string {
  if (n < 0) return "neg";
  else if (n === 0) return "zero";
  else if (n < 10) return "small";
  else if (n < 100) return "medium";
  else return "big";
}
print(bucket(-3));
print(bucket(0));
print(bucket(7));
print(bucket(42));
print(bucket(500));

// 3. Aninhado — outer + inner
function quadrant(x: i32, y: i32): string {
  if (x >= 0) {
    if (y >= 0) return "NE";
    else        return "SE";
  } else {
    if (y >= 0) return "NW";
    else        return "SW";
  }
}
print(quadrant(1, 1));
print(quadrant(1, -1));
print(quadrant(-1, 1));
print(quadrant(-1, -1));

// 4. Condicao composta — &&, ||, !
const a: i32 = 5;
const b: i32 = 10;
if (a > 0 && b > 0) print("both-pos");
if (a > 100 || b > 5) print("any-big");
if (!(a > 100)) print("not-too-big");

// 5. Sem else — branch nao executa quando false
let counter: i32 = 0;
if (false) counter = counter + 100;
if (true)  counter = counter + 1;
print("counter:" + counter);

// 6. Short-circuit — segundo operando nao avalia se primeiro decide
let calls: i32 = 0;
function bumpCalls(): boolean { calls = calls + 1; return true; }
if (false && bumpCalls()) {} // bumpCalls NAO chamado
if (true  || bumpCalls()) {} // bumpCalls NAO chamado
if (true  && bumpCalls()) {} // bumpCalls SIM
if (false || bumpCalls()) {} // bumpCalls SIM
print("calls:" + calls);     // 2

// 7. If como expressao via ternario tambem cobre — mas if-stmt em fn
//    com early-return e a forma idiomatica testada acima (bucket).

// 8. Else vinculado ao if mais proximo (dangling else)
function dangle(p: boolean, q: boolean): string {
  if (p)
    if (q) return "pq";
    else   return "p-not-q";
  return "outside";
}
print(dangle(true, true));
print(dangle(true, false));
print(dangle(false, false));

describe("if/else — chains, nested, short-circuit", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "big\nfive\n" +
      "neg\nzero\nsmall\nmedium\nbig\n" +
      "NE\nSE\nNW\nSW\n" +
      "both-pos\nany-big\nnot-too-big\n" +
      "counter:1\n" +
      "calls:2\n" +
      "pq\np-not-q\noutside\n"
    );
  });
});
