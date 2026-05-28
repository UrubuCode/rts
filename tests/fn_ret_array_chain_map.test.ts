import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): fn top-level retornando array (`mx(): number[][]`)
// chamada em chain direto `mx(2,3).map(...)` crashava SIGILL — collect_methods
// _ret_array / looks_array_call so' reconheciam METODOS de classe e methods de
// array, nao fns top-level com return_type array. Fix: estende a coleta p/
// Item::Function ret-array e os reconhecedores (init_looks_array +
// looks_array_call) p/ Call de Ident no set. Completa #1241/#1246.

let out = "";
function print(v: string): void { out += v + "\n"; }

function mx(rows: number, cols: number): number[][] {
  const m: number[][] = [];
  for (let i = 0; i < rows; i++) {
    const row: number[] = [];
    for (let j = 0; j < cols; j++) row.push(i * cols + j);
    m.push(row);
  }
  return m;
}
// chain direto sobre fn-ret-array
print(mx(2, 3).map(r => r.join("-")).join("|"));   // 0-1-2|3-4-5

function words(): string[] { return ["a", "b", "c"]; }
print(words().filter(w => w !== "b").join(""));     // ac
print(words().map(w => w.toUpperCase()).join(""));  // ABC

// via var continua OK
const w = words();
print(w.map(x => x + "!").join(""));                // a!b!c!

describe("fn ret array chain map", () => {
  test("fn top-level ret-array em chain direto", () =>
    expect(out).toBe("0-1-2|3-4-5\nac\nABC\na!b!c!\n"));
});
