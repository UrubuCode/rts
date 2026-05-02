import { describe, test, expect } from "rts:test";
import { io } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Switch denso (todos literais inteiros) — vira jump table
function label(n: i32): void {
  switch (n) {
    case 0: print("zero"); break;
    case 1: print("one"); break;
    case 2: print("two"); break;
    case 5: print("five"); break;
    case 10: print("ten"); break;
    default: print("other");
  }
}
label(0);
label(1);
label(2);
label(3);
label(5);
label(10);
label(99);
label(-1);

// 2. Fall-through (sem break) — passa para o proximo case
function vowelOrNot(c: i32): string {
  switch (c) {
    case 65: case 69: case 73: case 79: case 85:
      return "vogal";
    case 32:
      return "espaco";
    default:
      return "outro";
  }
}
print(vowelOrNot(65));    // 'A' = vogal
print(vowelOrNot(69));    // 'E' = vogal
print(vowelOrNot(74));    // 'J' = outro
print(vowelOrNot(32));    // ' ' = espaco

// 3. Switch com return (sem break) — saida imediata
function dayName(d: i32): string {
  switch (d) {
    case 1: return "seg";
    case 2: return "ter";
    case 3: return "qua";
    case 4: return "qui";
    case 5: return "sex";
    default: return "fim de semana";
  }
}
print(dayName(1));
print(dayName(5));
print(dayName(7));

// 4. Default no meio — comportamento JS (testa se fall-through pra cima funciona)
function tag(n: i32): string {
  let res: string = "";
  switch (n) {
    case 1: res = "um"; break;
    case 2: res = "dois"; break;
    default: res = "?"; break;
    case 3: res = "tres"; break;
  }
  return res;
}
print(tag(1));
print(tag(2));
print(tag(3));
print(tag(99));

// 5. Switch com expressao no scrutinee
function pair(a: i32, b: i32): string {
  switch (a + b) {
    case 0:  return "zero";
    case 10: return "dez";
    default: return "outro";
  }
}
print(pair(3, 7));    // dez
print(pair(-2, 2));   // zero
print(pair(1, 5));    // outro

// 6. Switch dentro de loop — counter por bucket
const inputs = [0, 1, 2, 1, 0, 5, 0];
let zeros: i32 = 0, ones: i32 = 0, fives: i32 = 0, others: i32 = 0;
for (const v of inputs) {
  switch (v) {
    case 0: zeros = zeros + 1; break;
    case 1: ones = ones + 1; break;
    case 5: fives = fives + 1; break;
    default: others = others + 1;
  }
}
print(`z=${zeros} o=${ones} f=${fives} x=${others}`);

describe("switch — fall-through, return, expression, in-loop", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "zero\none\ntwo\nother\nfive\nten\nother\nother\n" +
      "vogal\nvogal\noutro\nespaco\n" +
      "seg\nsex\nfim de semana\n" +
      "um\ndois\ntres\n?\n" +
      "dez\nzero\noutro\n" +
      "z=3 o=2 f=1 x=1\n"
    );
  });
});
