import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#1078/#341) Metodo de prototype (`Fn.prototype.m = namedFn`) que retorna
// f64 inequivoco (Math.sqrt / divisao / float lit) deve preservar o valor
// via INVOKE_AUTO_TYPED. Antes o trampolim invocava como `-> i64` e truncava
// (`circ.area()` -> 15 em vez de 15.7).
function Circle(this: any, r: number) { this.radius = r; }
function area(this: any): number { return 3.14 * this.radius; }
function diag(this: any): number { return Math.sqrt(this.radius * this.radius * 2); }
Circle.prototype.area = area;
Circle.prototype.diag = diag;

// Metodo que retorna INT puro NAO deve ser afetado (continua i64 correto).
function Box(this: any, n: number) { this.n = n; }
function inc(this: any): number { return this.n + 1; }
Box.prototype.inc = inc;

const c = new (Circle as any)(5);
print("area=" + c.area().toFixed(2));
print("diag=" + c.diag().toFixed(4));

const b = new (Box as any)(41);
print("inc=" + b.inc());

describe("proto method f64 return (#1078)", () => {
  test("f64 preservado, int intacto", () =>
    expect(out).toBe("area=15.70\ndiag=7.0711\ninc=42\n"));
});
