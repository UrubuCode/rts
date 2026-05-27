import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (336_prototype_chain / 387) Funcao-construtora plain com TS `this` param.
// O `this` param eh sintatico (vem do slot, nao dos args); descartado no
// parser para que os args reais alinhem com os params restantes. Antes,
// `new Shape("red")` passava "red" para o slot `this` e `color` ficava
// undefined.
function Shape(this: any, color: string) { this.color = color; }
Shape.prototype.describe = function(this: any) { return this.color + " shape"; };

function Circle(this: any, color: string, r: number) {
  Shape.call(this, color);
  this.radius = r;
}

const s = new (Shape as any)("red");
print("color=" + s.color);

const c = new (Circle as any)("blue", 7);
print("c.color=" + c.color);
print("c.radius=" + c.radius);

describe("fn-construtora this param (336/387)", () => {
  test("args alinham apos descartar this param", () =>
    expect(out).toBe("color=red\nc.color=blue\nc.radius=7\n"));
});
