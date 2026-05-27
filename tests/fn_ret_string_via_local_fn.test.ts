import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#270) fn que retorna `localFn()` onde localFn (arrow/fn) retorna string
// deve inferir retorno string (Handle) — antes o handle voltava como bits.
function wrapArrow() {
  const f = () => "viaArrow";
  return f();
}
function wrapFn() {
  const g = function() { return "viaFn"; };
  return g();
}
function viaString() {
  const s = () => String(123);
  return s();
}

print("a=" + wrapArrow());
print("b=" + wrapFn());
print("c=" + viaString());

describe("fn ret string via local fn (#270)", () => {
  test("return localFn() string-yielding infere Handle", () =>
    expect(out).toBe("a=viaArrow\nb=viaFn\nc=123\n"));
});
