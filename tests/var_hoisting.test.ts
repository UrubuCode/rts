import { describe, test, expect } from "rts:test";

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
  print(`${x}`);

  var x: i64 = 5;

  print(`${x}`);
}
fnHoist();

// `var i` em for: function-scoped — vive fora do loop.
function forVar(): void {
  for (var i: i64 = 0; i < 3; i = i + 1) {}
  print(`${i}`); // i === 3 fora do loop
}
forVar();

// var top-level
print(`${z}`);
var z: i64 = 99;
print(`${z}`);

describe("fixture:var_hoisting", () => {
  test("var declarations are hoisted to function/module scope (#301)", () => {
    expect(__rtsCapturedOutput).toBe("0\n5\n3\n0\n99\n");
  });
});
