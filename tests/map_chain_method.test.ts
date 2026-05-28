import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `mk().get(k)` / `mk().has(k)` chain direto
// (sem var intermediaria) CRASHAVA (SIGILL) quando mk() retorna Map/Set.
// mk() retorna i64 nao-tipado-Handle -> receiver caia no caminho
// number_builtin em vez de map_set_builtin. Fix: re-tipa o receiver como
// Handle quando mk eh fn em FNS_RET_MAPSET. (Continuacao de #1262.)

let out = "";
function print(v: string): void { out += v + "\n"; }

function mkMap(): Map<string, number> {
  const m = new Map<string, number>();
  m.set("a", 1);
  m.set("b", 2);
  return m;
}
print(mkMap().get("a") + "");        // 1
print(mkMap().has("b") + "");        // true
print(mkMap().has("z") + "");        // false

function mkSet(): Set<number> {
  const s = new Set<number>();
  s.add(5); s.add(6);
  return s;
}
print(mkSet().has(6) + "");          // true

// Map em array de Maps (mesma raiz: elemento nao-tipado-Handle)
const maps: Map<string, number>[] = [mkMap(), mkMap()];
print(maps[0].size + "");            // 2
print(maps[1].get("a") + "");        // 1

describe("Map/Set metodo em chain direto", () => {
  test("mk().get/has sem crash", () =>
    expect(out).toBe("1\ntrue\nfalse\ntrue\n2\n1\n"));
});
