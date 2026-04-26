// Teste do layout nativo opt-in (#147 passos 5-7).
//
// O codegen RTS usa o caminho `gc.instance_*` (memoria contigua, slots
// tipados) quando o nome da classe comeca com `__Flat` — gatilho hardcoded
// que dispensa env var (rts:test nao propaga `RTS_FLAT_CLASSES`). Classes
// que nao casam continuam usando o `Map`-based path antigo.
import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

class __FlatPoint {
  x: i32;
  y: i32;
  constructor(a: i32, b: i32) {
    this.x = a;
    this.y = b;
  }
}

const p: __FlatPoint = new __FlatPoint(3, 4);
print(`${p.x}`);
print(`${p.y}`);
p.x = 99;
print(`${p.x}`);

class __FlatVec {
  v: f64;
  constructor(a: f64) {
    this.v = a;
  }
}

const fv: __FlatVec = new __FlatVec(2.5);
print(`${fv.v}`);
fv.v = 7.25;
print(`${fv.v}`);

// Coexistencia: classe sem prefixo continua via HashMap path.
class RegularPoint {
  x: i32;
  y: i32;
  constructor(a: i32, b: i32) {
    this.x = a;
    this.y = b;
  }
}

const r: RegularPoint = new RegularPoint(11, 22);
print(`${r.x}`);
print(`${r.y}`);

describe("class_flat_layout", () => {
  test("flat classes round-trip por offset, regulares via map", () => {
    expect(__rtsCapturedOutput).toBe("3\n4\n99\n2.5\n7.25\n11\n22\n");
  });
});
