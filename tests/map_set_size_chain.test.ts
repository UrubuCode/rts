import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `mk().size` (chain direto, sem var intermediaria)
// dava 0 quando mk() retorna Map/Set. local_map_vars cobria so' Idents; o
// receiver Call nao era reconhecido. Fix: recv_is_mapvar reconhece Expr::Call
// de fn em FNS_RET_MAPSET. (Continuacao de #1260/#1261.)

let out = "";
function print(v: string): void { out += v + "\n"; }

function mkMap(): Map<string, number> {
  const m = new Map<string, number>();
  m.set("a", 1);
  m.set("b", 2);
  return m;
}
print(mkMap().size + "");        // 2

function mkSet(): Set<number> {
  const s = new Set<number>();
  s.add(1); s.add(2); s.add(3);
  return s;
}
print(mkSet().size + "");        // 3

// campo de classe continua OK (guard)
class Store {
  data: Map<string, number> = new Map<string, number>();
  add(k: string, v: number): void { this.data.set(k, v); }
  count(): number { return this.data.size; }
}
const st = new Store();
st.add("x", 1);
st.add("y", 2);
print(st.count() + "");          // 2
print(st.data.size + "");        // 2

describe("Map/Set size em chain direto", () => {
  test("mk().size sem var intermediaria", () =>
    expect(out).toBe("2\n3\n2\n2\n"));
});
