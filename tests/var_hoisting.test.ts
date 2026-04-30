import { describe, test, expect } from "rts:test";
import { gc } from "rts";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #301 fase 1: var hoisting.
//
// `var x` em qualquer ponto do body de uma fn (top-level ou user fn)
// deve ser visivel desde o inicio com valor 0 (proxy de undefined).
// `let`/`const` continuam block-scoped (TDZ ainda nao implementado —
// fica para fase 2).

function fnHoist(): void {
  // Le `x` antes do `var x = 5` — deve dar 0 (hoisted).
  const before = gc.string_from_i64(x);
  print(before); gc.string_free(before);

  var x: i64 = 5;

  const after = gc.string_from_i64(x);
  print(after); gc.string_free(after);
}
fnHoist();

// `var i` em for: function-scoped — vive fora do loop.
function forVar(): void {
  for (var i: i64 = 0; i < 3; i = i + 1) {}
  const h = gc.string_from_i64(i); // i === 3 fora do loop
  print(h); gc.string_free(h);
}
forVar();

// var top-level
const tlBefore = gc.string_from_i64(z);
print(tlBefore); gc.string_free(tlBefore);
var z: i64 = 99;
const tlAfter = gc.string_from_i64(z);
print(tlAfter); gc.string_free(tlAfter);

describe("fixture:var_hoisting", () => {
  test("var declarations are hoisted to function/module scope (#301)", () => {
    expect(__rtsCapturedOutput).toBe("0\n5\n3\n0\n99\n");
  });
});
