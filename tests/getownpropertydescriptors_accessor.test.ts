import { describe, test, expect } from "rts:test";

// (#749) Object.getOwnPropertyDescriptors deve emitir accessor descriptor
// (com get/set) para slots `__get_<name>` / `__set_<name>` em vez de
// vazar essas keys internas no output. Antes do fix, `descs.b.get` em
// um obj com `get b() {...}` segfaultava porque descs.b nao existia
// (so' __get_b existia como chave literal).

const obj: any = {
  a: 1,
  get b() { return 2; },
};

const descs: any = Object.getOwnPropertyDescriptors(obj);
const keys = (Object.keys(descs) as any).sort().join(",");
const aVal = descs.a.value;
const hasB = (descs.b !== undefined) ? "yes" : "no";
const bGetDefined = ((descs.b as any).get !== undefined) ? "yes" : "no";

describe("Object.getOwnPropertyDescriptors accessor pair (#749)", () => {
  test("keys nao vazam __get_/__set_", () => expect(keys).toBe("a,b"));
  test("data prop a.value", () => expect(aVal).toBe(1));
  test("descs.b existe", () => expect(hasB).toBe("yes"));
  test("accessor descs.b.get definido", () => expect(bGetDefined).toBe("yes"));
});
