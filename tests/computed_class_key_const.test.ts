import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#271) Computed class member key que eh const string literal:
// `const key = "sum"; class C { [key](a,b){return a+b} }`. Antes o metodo
// virava "key" (snippet) e `c.sum(...)` nao achava o metodo.
const mname = "compute";
const pname = "label";
class C {
  [mname](a: number, b: number) { return a * b; }
  [pname]() { return "hello"; }
}
const c = new C();
print("m=" + (c as any).compute(6, 7));
print("p=" + (c as any).label());

describe("computed class key const string (#271)", () => {
  test("metodo nomeado pelo valor da const", () =>
    expect(out).toBe("m=42\np=hello\n"));
});
