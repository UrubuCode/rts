import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `const c2 = inst.method(); c2.method()` onde o
// metodo de instancia retorna a propria classe (`inc(): C { ...; return this }`)
// crashava SIGILL — c2 nao era marcado com a classe C, entao `c2.inc()` caia
// no fallback MAP_GET("inc") -> trapz. Fix: decls marca a var receptora com a
// classe quando o metodo de instancia tem ret_class == classe do receiver.

let out = "";
function print(v: string): void { out += v + "\n"; }

class Counter {
  n: number = 0;
  inc(): Counter { this.n++; return this; }
  add(x: number): Counter { this.n += x; return this; }
  get value(): number { return this.n; }
}

// var intermediaria de metodo que retorna a classe
const c = new Counter();
const c2 = c.inc();
c2.inc();
print("" + c.value);   // 2 (mesmo objeto)

// encadeamento via vars
const d = new Counter();
const d1 = d.inc();
const d2 = d1.add(10);
d2.inc();
print("" + d.value);   // 1+10+1 = 12

describe("builder method chain var", () => {
  test("var de metodo ret-classe eh reconhecida", () =>
    expect(out).toBe("2\n12\n"));
});
