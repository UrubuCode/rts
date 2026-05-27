import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): builder com return type `: this` (em vez de
// `: C` explicito) — `inc(): this { return this }` — crashava no chain
// `c.inc().inc()` porque ret_class ficava None p/ "this". Fix: program.rs
// detecta return_type=="this" e extrai a classe owner do nome mangled
// __class_<C>_<method>, setando ret_class=C. Completa #1245/#1246.

let out = "";
function print(v: string): void { out += v + "\n"; }

class Counter {
  n: number = 0;
  inc(): this { this.n++; return this; }
  add(x: number): this { this.n += x; return this; }
  get value(): number { return this.n; }
}

// chain direto com : this
const c = new Counter();
c.inc().inc().inc();
print("" + c.value);   // 3

// chain misto
const d = new Counter();
d.inc().add(10).inc();
print("" + d.value);   // 12

// builder string com : this + build final
class B {
  items: string[] = [];
  add(s: string): this { this.items.push(s); return this; }
  build(): string { return this.items.join(","); }
}
print(new B().add("x").add("y").add("z").build());  // x,y,z

// via var
const b = new B();
const b2 = b.add("a");
b2.add("b");
print(b.build());   // a,b

describe("builder method chain this", () => {
  test("return : this resolve a classe", () =>
    expect(out).toBe("3\n12\nx,y,z\na,b\n"));
});
