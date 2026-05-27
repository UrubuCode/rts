import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Alias de array: `const es = arr` deve permitir `es.map(arrow)` igual a
// `arr.map(arrow)`. Antes, o arrow inline so' era liftado quando o receiver
// era reconhecido como array receiver; um alias direto nao entrava nesse
// conjunto e o codegen trapava (saida vazia / illegal instruction).
const arr = [1, 2, 3];
const es = arr;
const doubled = es.map((x: number) => x * 2);
print("doubled=" + doubled.join(","));

const arr2 = [10, 20, 30];
const alias2 = arr2;
const sum = alias2.map((x: number) => x + 1).join(",");
print("plus1=" + sum);

describe("array alias map(arrow)", () => {
  test("alias direto permite lift de arrow inline", () =>
    expect(out).toBe("doubled=2,4,6\nplus1=11,21,31\n"));
});
