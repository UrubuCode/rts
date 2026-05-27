import { describe, test, expect } from "rts:test";

let out: string = "";
function print(v: string): void { out += v + "\n"; }

// (#1217 / 373) Acesso a campo privado `#x` em receiver de tipo `unknown`.
// O private name eh lexicamente restrito a classe corrente, entao o tipo do
// campo (number = f64) deve ser inferido pela classe declarante mesmo quando
// o receiver estatico nao tem classe conhecida. Antes, `other.#x` com
// `other: unknown` lia o f64 como i64 (fcvt) e produzia lixo.
class Point {
  #x: number;
  #y: number;
  constructor(x: number, y: number) { this.#x = x; this.#y = y; }
  static isPoint(v: unknown): v is Point {
    return v !== null && typeof v === "object" && #x in (v as any);
  }
  distanceTo(other: unknown): number {
    if (!Point.isPoint(other)) throw new TypeError("not a Point");
    return Math.sqrt((this.#x - other.#x) ** 2 + (this.#y - other.#y) ** 2);
  }
}

const p1 = new Point(0, 0);
const p2 = new Point(3, 4);
const d = p1.distanceTo(p2);

print("d=" + d);
print("isPoint(p2)=" + Point.isPoint(p2));
print("isPoint({})=" + Point.isPoint({}));

describe("private field em receiver unknown (#1217)", () => {
  test("distanceTo retorna number, nao bits", () =>
    expect(out).toBe("d=5\nisPoint(p2)=true\nisPoint({})=false\n"));
});
