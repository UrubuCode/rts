import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#1078) Metodo f64 instalado via descriptor de Object.create:
// `Object.create(proto, { area: { value: function(){...f64...} } })`.
// O call site `c.area()` deve preservar o f64 (antes truncava).
function Circle(this: any, r: number) { this.radius = r; }
Circle.prototype = Object.create(Object.prototype, {
  area: { value: function(this: any): number { return Math.PI * this.radius * this.radius; }, writable: true, configurable: true },
});
const c = new (Circle as any)(5);
print("area=" + Math.round(c.area()));

describe("proto method f64 via descriptor (#1078)", () => {
  test("Object.create descriptor value:fn preserva f64", () =>
    expect(out).toBe("area=79\n"));
});
