import { describe, test, expect } from "rts:test";

// (#94) `for (const k in arr)` em Array retornava 0 iteracoes porque o
// codegen usava MAP_LEN/MAP_KEY_AT que so' funcionam em Entry::Map
// (retornam -1 em Vec). Fix: usar OBJECT_KEYS_AUTO que retorna Vec<i64>
// de handles de keys para Map E Vec, depois iterar via VEC_LEN/VEC_GET.

const arr = [10, 20, 30];
const arrKeys: string[] = [];
for (const k in arr) {
  arrKeys.push(k);
}

const obj: any = { a: 1, b: 2, c: 3 };
const objKeys: string[] = [];
for (const k in obj) {
  objKeys.push(k);
}

const empty: any[] = [];
const emptyKeys: string[] = [];
for (const k in empty) {
  emptyKeys.push(k);
}

describe("for-in em Array (#94)", () => {
  test("for-in em [10,20,30] gera '0','1','2'", () =>
    expect(arrKeys.join("|")).toBe("0|1|2"));
  test("for-in em Map continua funcionando", () =>
    expect(objKeys.join("|")).toBe("a|b|c"));
  test("for-in em array vazio nao itera", () =>
    expect(emptyKeys.length).toBe(0));
});
