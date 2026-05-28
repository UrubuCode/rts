import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): Map.size/Set.size davam 0 quando o Map/Set vinha
// de retorno de fn (`const m = mk(); m.size`). `.get`/`.has` funcionavam (handle
// resolvia) mas `.size` caia em MAP_GET("size")=0 pois a var nao era tipada
// Handle. Fix: local_map_vars marca a var (anotacao Map/Set, `new Map/Set`, ou
// fn que retorna Map/Set via FNS_RET_MAPSET) -> `.size` usa UNIVERSAL_LENGTH.

let out = "";
function print(v: string): void { out += v + "\n"; }

function mkMap(): Map<string, number> {
  const m = new Map<string, number>();
  m.set("a", 1);
  m.set("b", 2);
  return m;
}
const mm = mkMap();               // sem anotacao -> via FNS_RET_MAPSET
print(mm.size + "");              // 2
print(mm.get("b") + "");         // 2

const mm2: Map<string, number> = mkMap();  // anotacao explicita
print(mm2.size + "");            // 2

function mkSet(): Set<number> {
  const s = new Set<number>();
  s.add(1); s.add(2); s.add(3); s.add(2);
  return s;
}
const ss = mkSet();
print(ss.size + "");             // 3
print(ss.has(2) + "");           // true

// new Map/Set local continua OK (guard)
const local = new Map<string, number>();
local.set("x", 1);
print(local.size + "");          // 1

describe("Map/Set size retornado de fn", () => {
  test("size funciona em Map/Set de retorno e anotado", () =>
    expect(out).toBe("2\n2\n2\n3\ntrue\n1\n"));
});
