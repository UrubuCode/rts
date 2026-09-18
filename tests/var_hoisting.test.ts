import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// #301 fase 1: var hoisting.
//
// `var x` em qualquer ponto do body de uma fn (top-level ou user fn)
// deve ser visivel desde o inicio com valor `undefined`.
//
// A versao anterior esperava `0`, o "proxy de undefined" do motor antigo, que
// nao tinha `undefined` num local `i64`. A anotacao `i64` e apagada: o valor de
// um `var` antes da sua linha e `undefined` em qualquer motor — Bun 1.4 com
// este mesmo ficheiro imprime "undefined\n5\n3\nundefined\n99\n".

function fnHoist(): void {
  // Le `x` antes do `var x = 5` — deve dar undefined (hoisted).
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
    expect(__rtsCapturedOutput).toBe("undefined\n5\n3\nundefined\n99\n");
  });
});
