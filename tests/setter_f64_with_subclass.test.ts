import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (38_classes_deep) Setter de campo `number` (f64) numa classe com subclasse
// que herda/sobrescreve accessors. O assign `a.value = 12` ia por dispatch
// virtual de setter; o RHS f64 (bits) era reconvertido via fcvt_from_sint
// em vez de bitcast, lendo os bits como inteiro e gerando lixo.
class Base {
  label: string;
  _value: number;
  constructor(label: string, value: number) { this.label = label; this._value = value; }
  get value(): number { return this._value; }
  set value(n: number) { this._value = n; }
  bump(step: number): number { this._value += step; return this._value; }
  static seed(label: string): Base { return new Base(label, 1); }
}
class Double extends Base {
  bump(step: number): number { return super.bump(step * 2); }
}

const a = Base.seed("alpha");
print("a0=" + a.value);
print("a1=" + a.bump(4));
a.value = 12;
print("a2=" + a.value);

const b = new Double("beta", 5);
print("b0=" + b.bump(3));

describe("setter f64 com subclasse (38_classes_deep)", () => {
  test("assign via setter virtual nao corrompe f64", () =>
    expect(out).toBe("a0=1\na1=5\na2=12\nb0=11\n"));
});
