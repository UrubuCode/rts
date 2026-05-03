import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #432: ?.prop em obj null deve produzir "undefined" em template literal,
// nao "0".

const o1: { x: number } | null = null;
print(`a=${o1?.x}`);

// Mesma coisa em handle (sub-objeto null)
const o2: { sub: { y: number } | null } = { sub: null };
print(`b=${o2.sub?.y}`);

// Optional indexing
const arr: number[] | null = null;
print(`c=${arr?.[0]}`);

// Quando obj nao eh null, leitura normal
const o3 = { x: 7 };
print(`d=${o3?.x}`);

describe("?. produz 'undefined' em template literal (#432)", () => {
  test("null obj -> undefined; non-null -> valor real", () => {
    expect(__rtsCapturedOutput).toBe(
      "a=undefined\n" +
      "b=undefined\n" +
      "c=undefined\n" +
      "d=7\n"
    );
  });
});
