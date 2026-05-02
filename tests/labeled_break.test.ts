import { describe, test, expect } from "rts:test";
import { io, gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// 1. Labeled break sai do laco externo (caso historico)
{
  let pairs: i32 = 0;
  c1_outer: for (let i = 0; i < 5; i = i + 1) {
    for (let j = 0; j < 5; j = j + 1) {
      if (i == 2 && j == 3) {
        break c1_outer;
      }
      pairs = pairs + 1;
    }
  }
  // 5 + 5 + 3 = 13 (2 voltas inteiras + 3 da 3a)
  print("c1:" + pairs);
}

// 2. Tres niveis — break do middle a partir do interno
{
  let count: i32 = 0;
  c2_outer: for (let i = 0; i < 3; i = i + 1) {
    c2_middle: for (let j = 0; j < 3; j = j + 1) {
      for (let k = 0; k < 3; k = k + 1) {
        if (k == 1) break c2_middle;
        count = count + 1;
      }
      // nao executa
      count = count + 100;
    }
  }
  // 3 outer * 1 inc cada = 3
  print("c2:" + count);
}

// 3. break sem label (so do laco mais interno)
{
  let outer_iters: i32 = 0;
  let inner_iters: i32 = 0;
  for (let i = 0; i < 3; i = i + 1) {
    outer_iters = outer_iters + 1;
    for (let j = 0; j < 5; j = j + 1) {
      if (j == 2) break;
      inner_iters = inner_iters + 1;
    }
  }
  // outer roda 3, inner soma 2*3 = 6
  print("c3:" + outer_iters + ":" + inner_iters);
}

// 4. break label dentro de while aninhado — para tudo na primeira ocorrencia
{
  let i: i32 = 0;
  let total: i32 = 0;
  c4_loop: while (i < 5) {
    let j: i32 = 0;
    while (j < 5) {
      if (i + j > 4) break c4_loop;
      total = total + 1;
      j = j + 1;
    }
    i = i + 1;
  }
  // i=0: j=0..4 → 5 incs. i=1: j=0,1,2,3 (sum 4 ≤ 4), j=4 (5>4 break L1) → 4 incs.
  // total = 5 + 4 = 9
  print("c4:" + total);
}

// 5. break label em do-while (label em do-while externo)
{
  let n: i32 = 0;
  let runs: i32 = 0;
  c5_loop: do {
    let m: i32 = 0;
    do {
      runs = runs + 1;
      if (n == 2 && m == 1) break c5_loop;
      m = m + 1;
    } while (m < 3);
    n = n + 1;
  } while (n < 5);
  // n=0: m=0,1,2 (3 runs), n=1: m=0,1,2 (3 runs), n=2: m=0,1 break (2 runs)
  // total runs = 3 + 3 + 2 = 8
  print("c5:" + runs);
}

// 6. continue com label — itera proximo do externo
{
  let cells: i32 = 0;
  c6_outer: for (let i = 0; i < 4; i = i + 1) {
    for (let j = 0; j < 4; j = j + 1) {
      if (j === i) continue c6_outer;  // pula resto da linha
      cells = cells + 1;
    }
  }
  // i=0: j=0 continue → 0 cells
  // i=1: j=0 (1), j=1 continue → 1 cell
  // i=2: j=0,1 (2), j=2 continue → 2 cells
  // i=3: j=0,1,2 (3), j=3 continue → 3 cells
  // total = 0+1+2+3 = 6
  print("c6:" + cells);
}

describe("labeled break/continue — niveis, while, do-while", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "c1:13\nc2:3\nc3:3:6\nc4:9\nc5:8\nc6:6\n"
    );
  });
});
