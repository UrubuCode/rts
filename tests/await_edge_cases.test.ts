import { describe, test, expect } from "rts:test";
import { promise } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

async function id(x: i64): i64 { return x; }

// 1. await dentro de logical AND/OR.
const a = (await id(1)) && (await id(2));
print("and=" + a);

const b = (await id(0)) || (await id(7));
print("or=" + b);

// 2. await com comparacoes.
const c = (await id(10)) > (await id(5));
print("gt=" + (c ? 1 : 0));

const d = (await id(3)) === (await id(3));
print("eq=" + (d ? 1 : 0));

// 3. await dentro de operacao bit a bit.
const e = (await id(12)) & (await id(10));  // 1100 & 1010 = 1000 = 8
print("and_bits=" + e);

const f = (await id(5)) | (await id(3));    // 101 | 011 = 111 = 7
print("or_bits=" + f);

// 4. await com unary minus.
const g = -(await id(42));
print("neg=" + g);

// 5. await em chamada com varios args.
function add3(x: i64, y: i64, z: i64): i64 { return x + y + z; }
const h = add3(await id(1), await id(2), await id(3));
print("add3=" + h);

// 6. await em chained calls (resultado nao-Promise).
function ident(x: i64): i64 { return x; }
const k = ident(ident(await id(99)));
print("nested=" + k);

// 7. await em parens explicitas.
const l = ((await id(50)));
print("paren=" + l);

// 8. await dentro de if-else aninhado.
async function classify(x: i64): i64 { return x > 50 ? 1 : 0; }
let label: i64 = -1;
if (await classify(75)) {
  label = 100;
} else {
  label = 200;
}
print("if_await=" + label);

// 9. multiple awaits em block.
let acc: i64 = 0;
{
  acc = acc + (await id(1));
  acc = acc + (await id(2));
  acc = acc + (await id(3));
}
print("block_acc=" + acc);

// 10. await em condicao de while (limita iteracoes).
let cnt: i64 = 0;
let iters: i64 = 0;
while ((await id(cnt)) < 5) {
  cnt = cnt + 1;
  iters = iters + 1;
}
print("while_iters=" + iters);

describe("await edge cases", () => {
  test("matches expected stdout", () => {
    expect(__rtsCapturedOutput).toBe(
      "and=2\nor=7\ngt=1\neq=1\nand_bits=8\nor_bits=7\nneg=-42\nadd3=6\nnested=99\nparen=50\nif_await=100\nblock_acc=6\nwhile_iters=5\n"
    );
  });
});
