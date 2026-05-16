import { describe, test, expect } from "rts:test";

// (#83) `"key" in obj` em arrow callback de Array.map falhava com
// "unsupported binary op: in". Causa: o path `BinaryOp::In` exigia
// obj_tv.ty == Handle, mas callback params vem como I64 raw carregando
// handle.

const arr: any[] = [{ value: 1 }, { other: 2 }];
const m = arr.map((y) => "value" in y ? "v" : "n").join(",");

describe("in operator em callback (#83)", () => {
  test("'value' in y dentro de map", () => expect(m).toBe("v,n"));
});
