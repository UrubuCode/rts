import { describe, test, expect } from "rts:test";

// (#341/elo-f64) `const ps = [new P(1.5), ...]` SEM anotacao `: P[]` nao
// inferia a classe do elemento. Consequencia: o bind de for-of e o param do
// callback liftado (filter/some/every/find) nao herdavam a classe -> o campo
// float `p.age` era lido como bits-i64, e `p.age >= 18` comparava bits (icmp)
// em vez de fcmp, contando floats truncados. Fix: inferir a classe-elemento
// do literal de `new C()` (decls.rs p/ for-of; ARRAY_ELEM_CLASS p/ o lift).

class P {
  age: number;
  name: string;
  constructor(age: number, name: string) {
    this.age = age;
    this.name = name;
  }
}

const ps = [new P(25.5, "A"), new P(17.0, "B"), new P(30.2, "C")];

// --- for-of com campo float (sem anotacao) ---
let forofCount = 0;
for (const p of ps) {
  if (p.age >= 18) { forofCount = forofCount + 1; }
}
let forofSum = 0;
for (const p of ps) { forofSum = forofSum + p.age; }

// --- callbacks liftados (predicate/comparison) ---
const filtered = ps.filter(p => p.age >= 18).length;
const someOld = ps.some(p => p.age > 30);
const everyAdult = ps.every(p => p.age > 10);
const found = ps.find(p => p.age < 20);

// --- heranca: campo herdado da superclasse ---
class A { v: number; constructor(v: number) { this.v = v; } }
class B extends A { constructor(v: number) { super(v); } }
const bs = [new B(1.5), new B(2.5), new B(3.5)];
const inheritCount = bs.filter(b => b.v >= 2).length;

// --- int field continua correto (nao-regressao) ---
class C { n: number; constructor(n: number) { this.n = n; } }
const cs = [new C(10), new C(20), new C(30)];
const intCount = cs.filter(c => c.n > 15).length;

describe("array de objetos com campo float (#341)", () => {
  test("for-of filtra float (>= 18)", () => expect(`${forofCount}`).toBe("2"));
  test("for-of soma campo float", () => expect(forofSum.toFixed(1)).toBe("72.7"));
  test("filter predicate float", () => expect(`${filtered}`).toBe("2"));
  test("some campo float", () => expect(someOld).toBe(true));
  test("every campo float", () => expect(everyAdult).toBe(true));
  test("find campo float retorna match", () => expect(found.name).toBe("B"));
  test("heranca: campo da superclasse", () => expect(`${inheritCount}`).toBe("2"));
  test("int field nao-regressao", () => expect(`${intCount}`).toBe("2"));
});
