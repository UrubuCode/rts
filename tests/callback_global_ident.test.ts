import { describe, test, expect } from "rts:test";

// (#394) Callbacks de array methods que referenciam classes globais
// construtoras (`v instanceof Set`, `new Array(v)`, `Array.isArray(v)`)
// NAO devem tratar o nome global como captura de escopo. Antes, `Set`/
// `Map`/etc faltavam em is_known_global_ident -> viravam captura ->
// parallel.filter_bound nao resolvia o "local" `Set` -> erro de dispatch.
const mixed: any[] = [1, "x", [2, 3], { k: 4 }];

const arrays = mixed.filter((v) => Array.isArray(v));
const arraysLen = arrays.length; // 1

const nums = [1, 2, 3];
const sizes = nums.map((v) => new Array(v).length).join(","); // 1,2,3

// instanceof com classe global no callback (nao crasha).
const things: any[] = [1, 2, 3];
const sets = things.filter((v) => v instanceof Set);
const setsLen = sets.length; // 0

describe("callback_global_ident (#394)", () => {
  test("filter Array.isArray", () => expect(arraysLen).toBe(1));
  test("map new Array(v).length", () => expect(sizes).toBe("1,2,3"));
  test("filter instanceof Set nao crasha", () => expect(setsLen).toBe(0));
});
