import { describe, test, expect } from "rts:test";
import { serialize, deserialize } from "rts:serde";
import { writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// ── RTSP FORMAT FREEZE, TWO VERSIONS ────────────────────────────────────────
// GOLDEN_V1 is the exact byte stream `serialize(buildGraph())` produced when
// the v1 format shipped, on the OLD engine. It is kept as the v1 READER's
// test: decoding it proves a stream an earlier engine wrote still loads here
// (a CRIPTA `MetaSave` is one), and every assertion that read it before still
// reads it now.
//
// GOLDEN_V2 is what the current encoder writes for the same graph, and the
// byte-identity half of the freeze moved to it — the test's own instruction
// for an intentional format change: bump the version, keep v1 decoding, and
// regenerate the golden for the new version. v2 is not v1 with a new number:
// it has a string table, Map/Set opcodes and module-qualified class names
// (docs/engine/pickle.md), so no v2 encoder can reproduce v1's bytes, and
// asserting that it did would be asserting the old engine's heap layout.
// If a v2 test here fails after an intentional change, bump VERSION in
// crates/rts-core/src/entry/pickle/format.rs, keep v1 and v2 decoding, and
// regenerate GOLDEN_V2 — never just update the bytes to make it pass.

class GAnimal {
  nome: string;
  constructor(n: string) {
    this.nome = n;
  }
  som(): string {
    return "...";
  }
}
class GCao extends GAnimal {
  raca: string;
  constructor(n: string, r: string) {
    super(n);
    this.raca = r;
  }
  som(): string {
    return "au au";
  }
}
function gDobro(x: number): number {
  return x * 2;
}

// EXACTLY the construction order that produced GOLDEN.
function buildGraph(): any {
  const shared: any = { id: 42 };
  const root: any = {
    titulo: "golden v1",
    n: 3.5,
    i: 7,
    flag: true,
    nada: null,
    indef: undefined,
    lista: [1, "dois", false, shared],
    outra: shared,
    quando: new Date(1700000000000),
    padrao: /ab+c/gi,
    erro: new Error("golden boom"),
    mapa: new Map<string, number>(),
    conjunto: new Set<string>(),
    pet: new GCao("Rex", "vira-lata"),
    fn: gDobro,
  };
  root.mapa.set("a", 1);
  root.mapa.set("b", 2);
  root.conjunto.add("x");
  root.conjunto.add("y");
  root.ciclo = root;
  return root;
}

const GOLDEN_V1: number[] = [82,84,83,80,1,10,16,6,116,105,116,117,108,111,1,110,1,105,4,102,108,97,103,4,110,97,100,97,5,105,110,100,101,102,5,108,105,115,116,97,5,111,117,116,114,97,6,113,117,97,110,100,111,6,112,97,100,114,97,111,4,101,114,114,111,4,109,97,112,97,8,99,111,110,106,117,110,116,111,3,112,101,116,2,102,110,5,99,105,99,108,111,7,9,103,111,108,100,101,110,32,118,49,5,0,0,0,0,0,0,12,64,5,0,0,0,0,0,0,28,64,3,1,0,9,4,5,0,0,0,0,0,0,240,63,7,4,100,111,105,115,2,10,1,2,105,100,5,0,0,0,0,0,0,69,64,8,4,19,4,68,97,116,101,8,0,104,229,207,139,1,0,0,19,6,82,101,103,69,120,112,22,4,0,0,0,97,98,43,99,2,0,0,0,103,105,0,0,0,0,0,0,0,0,21,5,69,114,114,111,114,4,7,109,101,115,115,97,103,101,4,110,97,109,101,5,115,116,97,99,107,5,99,97,117,115,101,7,11,103,111,108,100,101,110,32,98,111,111,109,7,5,69,114,114,111,114,0,0,21,3,77,97,112,5,5,35,107,101,121,115,5,35,118,97,108,115,2,35,104,3,35,110,120,5,35,109,97,115,107,9,2,7,1,97,7,1,98,9,2,5,0,0,0,0,0,0,240,63,5,0,0,0,0,0,0,0,64,9,8,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,0,0,6,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,9,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,28,64,21,3,83,101,116,4,6,35,105,116,101,109,115,2,35,104,3,35,110,120,5,35,109,97,115,107,9,2,7,1,120,7,1,121,9,8,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,6,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,0,0,9,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,28,64,21,4,71,67,97,111,2,4,110,111,109,101,4,114,97,99,97,7,3,82,101,120,7,9,118,105,114,97,45,108,97,116,97,22,6,103,68,111,98,114,111,8,0];

// Written by the v2 encoder (RTSP version byte 2) for the same graph.
const GOLDEN_V2: number[] = [82,84,83,80,2,10,16,0,6,116,105,116,117,108,111,0,1,110,0,1,105,0,4,102,108,97,103,0,4,110,97,100,97,0,5,105,110,100,101,102,0,5,108,105,115,116,97,0,5,111,117,116,114,97,0,6,113,117,97,110,100,111,0,6,112,97,100,114,97,111,0,4,101,114,114,111,0,4,109,97,112,97,0,8,99,111,110,106,117,110,116,111,0,3,112,101,116,0,2,102,110,0,5,99,105,99,108,111,7,0,9,103,111,108,100,101,110,32,118,49,5,0,0,0,0,0,0,12,64,6,14,3,1,0,9,4,6,2,7,0,4,100,111,105,115,2,10,1,0,2,105,100,6,84,0,8,2,19,4,68,97,116,101,8,0,104,229,207,139,1,0,0,19,6,82,101,103,69,120,112,22,4,0,0,0,97,98,43,99,2,0,0,0,103,105,0,0,0,0,0,0,0,0,14,0,0,5,69,114,114,111,114,1,7,0,11,103,111,108,100,101,110,32,98,111,111,109,0,23,2,7,0,1,97,6,2,7,0,1,98,6,4,24,2,7,0,1,120,7,0,1,121,21,0,0,0,4,71,67,97,111,0,2,0,4,110,111,109,101,0,4,114,97,99,97,7,0,3,82,101,120,7,0,9,118,105,114,97,45,108,97,116,97,22,26,0,6,103,68,111,98,114,111,8,0];

// (a) format freeze: fresh serialize reproduces the v2 golden bytes exactly.
const fresh: any = serialize(buildGraph());
let sameLen = fresh.length === GOLDEN_V2.length;
let firstDiff = -1;
if (sameLen) {
  for (let i = 0; i < GOLDEN_V2.length; i++) {
    if (fresh[i] !== GOLDEN_V2[i]) {
      firstDiff = i;
      break;
    }
  }
}

// (b) cross-process contract: the v1 golden stream (produced by an EARLIER
// process, on the old engine) revives completely here — the v1 reader.
const g: any = deserialize(GOLDEN_V1);

// (b') the same contract for v2: the v2 golden revives, not only re-encodes.
const g2: any = deserialize(GOLDEN_V2);

// (c) disk round-trip: serialize → file → read → deserialize.
// `join`, not a literal separator: concatenating "\\" produced
// `/tmp\claude-pickle-golden.rtsp` on the ubuntu/macos runners — one filename
// containing a backslash, which the open then rejected (EACCES), crashing the
// whole file. `join` picks the platform separator.
const diskPath = join(tmpdir(), "claude-pickle-golden.rtsp");
writeFileSync(diskPath, serialize(buildGraph()) as any);
const fromDisk: any = deserialize(readFileSync(diskPath) as any);

describe("rts:serde golden (v2 format freeze + v1 and v2 cross-process)", () => {
  test("fresh serialize is byte-identical to the v2 golden", () => {
    expect(sameLen).toBe(true);
    expect(firstDiff).toBe(-1);
  });

  test("v2 golden: revives with identity, class and fn by reference", () => {
    expect(g2.titulo).toBe("golden v1");
    expect(g2.ciclo === g2).toBe(true);
    expect(g2.lista[3] === g2.outra).toBe(true);
    expect(g2.pet instanceof GCao).toBe(true);
    expect(g2.fn(21)).toBe(42);
  });

  test("v1 golden: primitives", () => {
    expect(g.titulo).toBe("golden v1");
    expect(g.n).toBe(3.5);
    expect(g.i).toBe(7);
    expect(g.flag).toBe(true);
    expect(g.nada).toBe(null);
    expect(g.indef).toBe(undefined);
  });

  test("v1 golden: array + shared identity + cycle", () => {
    expect(g.lista.length).toBe(4);
    expect(g.lista[1]).toBe("dois");
    expect(g.lista[3] === g.outra).toBe(true);
    expect(g.outra.id).toBe(42);
    expect(g.ciclo === g).toBe(true);
  });

  test("v1 golden: Date + RegExp + Error", () => {
    expect(g.quando.getTime()).toBe(1700000000000);
    expect(g.padrao.source).toBe("ab+c");
    expect(g.padrao.flags).toBe("gi");
    expect(g.erro.message).toBe("golden boom");
  });

  test("v1 golden: Map + Set revive", () => {
    expect(g.mapa instanceof Map).toBe(true);
    expect(g.mapa.get("b")).toBe(2);
    expect(g.conjunto instanceof Set).toBe(true);
    expect(g.conjunto.has("y")).toBe(true);
  });

  test("v1 golden: class instance + inheritance + fn by reference", () => {
    expect(g.pet instanceof GCao).toBe(true);
    expect(g.pet instanceof GAnimal).toBe(true);
    expect(g.pet.som()).toBe("au au");
    expect(g.fn(21)).toBe(42);
  });

  test("disk round-trip via node:fs", () => {
    expect(fromDisk.titulo).toBe("golden v1");
    expect(fromDisk.ciclo === fromDisk).toBe(true);
    expect(fromDisk.pet instanceof GCao).toBe(true);
    expect(fromDisk.mapa.get("a")).toBe(1);
  });
});
