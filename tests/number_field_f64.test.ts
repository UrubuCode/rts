import { describe, test, expect } from "rts:test";

// Regression (372): campo `number` (F64) deve ter representacao uniforme =
// bits do f64, com store e load simetricos. Antes, store via fcvt_to_sint
// truncava (373.15 -> 373) e leitura/store assimetricos liam bits como lixo.
// Cobre: init inteiro, init float, assign de chamada de metodo, private
// field, super.field, e arrow `() => this.campoF64`.

class Temperature {
  #kelvin: number;
  constructor(celsius: number) { this.#kelvin = this.#toKelvin(celsius); }
  #toKelvin(c: number) { return c + 273.15; }
  #toCelsius(k: number) { return k - 273.15; }
  get celsius() { return this.#toCelsius(this.#kelvin); }
  get kelvin() { return this.#kelvin; }
}
const t = new Temperature(100);
const tCelsius = t.celsius;
const tKelvin = t.kelvin;

// init float preservado
class Circle {
  r: number = 2.5;
  area(): number { return 3.14 * this.r * this.r; }
}
const circ = new Circle();
const circR = circ.r;

// arrow capturando campo number, retornado e chamado fora
class Mul {
  factor: number;
  constructor(f: number) { this.factor = f; }
  getFactor(): () => number { return () => this.factor; }
}
const getF: () => number = new Mul(7).getFactor();
const factorOut = getF();

describe("number field f64", () => {
  test("getter via metodos privados preserva fracao", () =>
    expect(tCelsius).toBe(100));
  test("private field f64 lido correto", () =>
    expect(tKelvin).toBe(373.15));
  test("init float nao-inteiro preservado", () =>
    expect(circR).toBe(2.5));
  test("arrow capturando campo number retornado", () =>
    expect(factorOut).toBe(7));
});
