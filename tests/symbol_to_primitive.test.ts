import { describe, test, expect } from "rts:test";

// (#216/274) `[Symbol.toPrimitive](hint)` invocado na coercao:
//   +obj          -> hint "number"
//   String(obj)   -> hint "string"
//   obj + ""      -> hint "default"
// O metodo recebe o hint e seu retorno eh usado como valor primitivo.
const obj: any = {
  [Symbol.toPrimitive](hint: string) {
    if (hint === "number") return 7;
    if (hint === "string") return "str!";
    return "def!";
  },
};

const numHint = +obj;            // 7
const strHint = String(obj);     // "str!"
const defHint = (obj as any) + ""; // "def!"

// objeto SEM toPrimitive continua "[object Object]" no default.
const plain: any = { a: 1 };
const plainConcat = (plain as any) + "";

describe("symbol_to_primitive (#274)", () => {
  test("hint number via +obj", () => expect(numHint).toBe(7));
  test("hint string via String(obj)", () => expect(strHint).toBe("str!"));
  test("hint default via obj + ''", () => expect(defHint).toBe("def!"));
  test("objeto sem toPrimitive", () => expect(plainConcat).toBe("[object Object]"));
});
