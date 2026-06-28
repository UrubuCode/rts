import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// (#264 PR3) Cenario realista: lista de objetos construidos via `new`,
// processados em loop. Simula um carrinho de compras.

function Item(price: number, qty: number): void {
  this.price = price;
  this.qty = qty;
  this.subtotal = price * qty;
}

const i1: any = new (Item as any)(10, 3);
const i2: any = new (Item as any)(25, 2);
const i3: any = new (Item as any)(7, 5);

// Soma subtotais
let total: number = 0;
total = total + (i1.subtotal as number);
total = total + (i2.subtotal as number);
total = total + (i3.subtotal as number);
print("total=" + total);  // 30 + 50 + 35 = 115

// Soma quantidades
let qtyTotal: number = 0;
qtyTotal = qtyTotal + (i1.qty as number);
qtyTotal = qtyTotal + (i2.qty as number);
qtyTotal = qtyTotal + (i3.qty as number);
print("qty=" + qtyTotal);  // 3+2+5 = 10

// Modificar uma instancia depois de criada — `this` slot ja foi popado
// entao writes diretos no handle Map continuam funcionando.
i2.qty = 99;
const newQty: number = i2.qty;
print("i2.qty after=" + newQty);

// As outras instancias nao sao afetadas (Maps independentes)
const i1Qty: number = i1.qty;
const ok: boolean = (i1Qty === 3);
print("i1.qty unchanged=" + ok);

describe("new UserFn cenario carrinho (#264 PR3)", () => {
  test("multiplas instances + agregacao + mutacao isolada", () =>
    expect(__rtsCapturedOutput).toBe(
      "total=115\n" +
      "qty=10\n" +
      "i2.qty after=99\n" +
      "i1.qty unchanged=true\n"
    ));
});
