import { describe, test, expect } from "rts:test";

// `x instanceof Error` dentro de callback de Array.map sem anotacao de
// tipo no param caia em `lower_global_instanceof` -> primitive check
// (lhs.ty == I64, nao Handle) -> retornava sempre false. JS spec: o
// runtime check de Entry::ErrorObj deve rodar mesmo quando o codegen
// infere I64 (array elements vem como i64 raw carregando handle).

const err = new Error("a");
const directly = err instanceof Error;

const arr: any[] = [new Error("a"), "b"];
const items = arr.map((x: any) => x instanceof Error ? x.message : String(x)).join("|");

describe("instanceof em callback param sem tipo", () => {
  test("instanceof direto continua funcionando", () => expect(directly).toBe(true));
  test("arr.map(x => x instanceof Error)", () => expect(items).toBe("a|b"));
});
