import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// Inferência de tipo de retorno: fn cujo `return <ident>` referencia uma var
// local inicializada com expr string-yielding (String(x), .join, etc.) deve
// inferir retorno Handle/string. Antes inferia Number e o handle voltava
// como bits crus.
function viaString(): string {
  const s = String(123);
  return s;
}
function viaJoin() {
  const t = [1, 2, 3].join("-");
  return t;
}
function viaConcat() {
  const u = "a" + "b";
  return u;
}

print("s=" + viaString());
print("j=" + viaJoin());
print("c=" + viaConcat());

describe("fn ret string via var", () => {
  test("infere retorno string de var local", () =>
    expect(out).toBe("s=123\nj=1-2-3\nc=ab\n"));
});
