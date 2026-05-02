import { describe, test, expect } from "rts:test";
import { io, i32 } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Caso classico — sum 0..4
{
  let i: i32 = 0;
  let acc: i32 = 0;
  while (i < 5) {
    acc = acc + i;
    i = i + 1;
  }
  print("acc:" + acc);  // 10
}

// 2. Condicao falsa de cara — body nao executa
{
  let runs: i32 = 0;
  while (false) runs = runs + 1;
  print("never:" + runs);  // 0
}

// 3. break sai do laco
{
  let i: i32 = 0;
  let sum: i32 = 0;
  while (true) {
    if (i >= 5) break;
    sum = sum + i;
    i = i + 1;
  }
  print("brk:" + sum);  // 10
}

// 4. continue pula iteracao
{
  let i: i32 = 0;
  let evens: i32 = 0;
  while (i < 10) {
    i = i + 1;
    if (i % 2 !== 0) continue;
    evens = evens + 1;
  }
  print("evens:" + evens);  // 5
}

// 5. Laco aninhado — break afeta so o interno
{
  let outer: i32 = 0;
  let pairs: i32 = 0;
  while (outer < 3) {
    let inner: i32 = 0;
    while (inner < 5) {
      if (inner === 2) break;
      pairs = pairs + 1;
      inner = inner + 1;
    }
    outer = outer + 1;
  }
  print("pairs:" + pairs);  // 3*2 = 6
}

// 6. do-while sempre executa pelo menos uma vez
{
  let n: i32 = 0;
  let count: i32 = 0;
  do {
    count = count + 1;
    n = n + 1;
  } while (n < 0);
  print("do-once:" + count);  // 1
}

// 7. do-while com varias iteracoes
{
  let n: i32 = 5;
  let sum: i32 = 0;
  do {
    sum = sum + n;
    n = n - 1;
  } while (n > 0);
  print("do-sum:" + sum);  // 15
}

// 8. while invertido (decrescente) com break em condicao composta
{
  let i: i32 = 100;
  let hits: i32 = 0;
  while (i > 0) {
    if (i % 25 === 0) hits = hits + 1;
    i = i - 1;
  }
  print("hits25:" + hits);  // 25, 50, 75, 100 = 4
}

describe("while/do-while — break, continue, nested, do-once", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "acc:10\nnever:0\nbrk:10\nevens:5\npairs:6\ndo-once:1\ndo-sum:15\nhits25:4\n"
    );
  });
});
