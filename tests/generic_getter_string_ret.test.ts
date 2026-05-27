import { describe, test, expect } from "rts:test";

// Getter com retorno generico T instanciado como string: handle de string
// deve formatar como texto, nao numero cru (irmao de generic_method_string_ret).
class Box<T> {
  private v: T;
  constructor(v: T) { this.v = v; }
  get value(): T { return this.v; }
}
const bs = new Box<string>("hi");
const s = "g=" + bs.value;

// Getter numerico permanece numero (sem regressao).
class Counter {
  private n: number = 0;
  inc(): void { this.n++; }
  get count(): number { return this.n; }
}
const c = new Counter();
c.inc(); c.inc();
const num = c.count;
const arith = c.count + 5;
const numStr = "c=" + c.count;

describe("generic getter string return", () => {
  test("T=string getter formata como texto", () => expect(s).toBe("g=hi"));
  test("getter numerico retorna numero", () => expect(num).toBe(2));
  test("aritmetica intacta", () => expect(arith).toBe(7));
  test("numero formata como numero", () => expect(numStr).toBe("c=2"));
});
