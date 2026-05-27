import { describe, test, expect } from "rts:test";

// Metodo com retorno generico T instanciado como string: o valor eh um
// handle de string e deve formatar como texto, nao como numero cru.
class Box<T> {
  private data: T[] = [];
  add(v: T): void { this.data.push(v); }
  take(): T | undefined { return this.data.pop(); }
}
const bs = new Box<string>();
bs.add("hello");
const s = "v=" + bs.take();

// Metodo numerico permanece numero (sem regressao em aritmetica/format).
class Counter {
  private n: number = 0;
  inc(): void { this.n++; }
  get(): number { return this.n; }
}
const c = new Counter();
c.inc(); c.inc();
const num = c.get();
const arith = c.get() + 10;
const numStr = "n=" + c.get();

describe("generic method string return", () => {
  test("T=string handle formata como texto", () => expect(s).toBe("v=hello"));
  test("metodo numerico retorna numero", () => expect(num).toBe(2));
  test("aritmetica intacta", () => expect(arith).toBe(12));
  test("numero formata como numero", () => expect(numStr).toBe("n=2"));
});
