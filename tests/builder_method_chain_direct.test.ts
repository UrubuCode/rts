import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): chain DIRETO de builder `c.inc().inc()` onde o
// metodo retorna a propria classe (`inc(): C { return this }`) crashava SIGILL
// — class_of_expr nao resolvia a classe do receiver `c.inc()` (Call), entao o
// 2o `.inc` caia no fallback MAP_GET -> trapz. Fix: class_of_expr resolve
// metodo de instancia de classe de usuario com ret_class == classe do receiver
// (builder genuino, restritivo p/ nao regredir). Complementa #1245 (caso var).

let out = "";
function print(v: string): void { out += v + "\n"; }

class Counter {
  n: number = 0;
  inc(): Counter { this.n++; return this; }
  add(x: number): Counter { this.n += x; return this; }
  get value(): number { return this.n; }
}

const c = new Counter();
c.inc().inc().inc();
print("" + c.value);   // 3

const d = new Counter();
d.inc().add(10).inc();
print("" + d.value);   // 12

// builder com build final (string)
class Builder {
  parts: string[] = [];
  add(s: string): Builder { this.parts.push(s); return this; }
  build(): string { return this.parts.join("-"); }
}
print(new Builder().add("a").add("b").add("c").build());  // a-b-c

describe("builder method chain direct", () => {
  test("chain direto de metodo ret-classe nao crasha", () =>
    expect(out).toBe("3\n12\na-b-c\n"));
});
