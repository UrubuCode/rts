import { describe, test, expect } from "rts:test";
import { serialize, deserialize } from "rts:serde";
import { writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";

// ── RTSP v1 FORMAT FREEZE ───────────────────────────────────────────────────
// GOLDEN is the exact byte stream `serialize(buildGraph())` produced when the
// v1 format shipped. Decoding it proves a stream from ANOTHER process/run
// loads (the pickle contract); re-encoding byte-for-byte proves no silent
// format drift. If a test here fails after an intentional format change, bump
// the wire VERSION in rts-engine/src/heap/pickle/mod.rs, keep the v1 decode
// path working, and regenerate GOLDEN for the new version — never just update
// the bytes to make it pass.

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

const GOLDEN: number[] = [82,84,83,80,1,10,16,6,116,105,116,117,108,111,1,110,1,105,4,102,108,97,103,4,110,97,100,97,5,105,110,100,101,102,5,108,105,115,116,97,5,111,117,116,114,97,6,113,117,97,110,100,111,6,112,97,100,114,97,111,4,101,114,114,111,4,109,97,112,97,8,99,111,110,106,117,110,116,111,3,112,101,116,2,102,110,5,99,105,99,108,111,7,9,103,111,108,100,101,110,32,118,49,5,0,0,0,0,0,0,12,64,5,0,0,0,0,0,0,28,64,3,1,0,9,4,5,0,0,0,0,0,0,240,63,7,4,100,111,105,115,2,10,1,2,105,100,5,0,0,0,0,0,0,69,64,8,4,19,4,68,97,116,101,8,0,104,229,207,139,1,0,0,19,6,82,101,103,69,120,112,22,4,0,0,0,97,98,43,99,2,0,0,0,103,105,0,0,0,0,0,0,0,0,21,5,69,114,114,111,114,4,7,109,101,115,115,97,103,101,4,110,97,109,101,5,115,116,97,99,107,5,99,97,117,115,101,7,11,103,111,108,100,101,110,32,98,111,111,109,7,5,69,114,114,111,114,0,0,21,3,77,97,112,5,5,35,107,101,121,115,5,35,118,97,108,115,2,35,104,3,35,110,120,5,35,109,97,115,107,9,2,7,1,97,7,1,98,9,2,5,0,0,0,0,0,0,240,63,5,0,0,0,0,0,0,0,64,9,8,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,0,0,6,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,9,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,28,64,21,3,83,101,116,4,6,35,105,116,101,109,115,2,35,104,3,35,110,120,5,35,109,97,115,107,9,2,7,1,120,7,1,121,9,8,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,6,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,0,0,9,2,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,240,191,5,0,0,0,0,0,0,28,64,21,4,71,67,97,111,2,4,110,111,109,101,4,114,97,99,97,7,3,82,101,120,7,9,118,105,114,97,45,108,97,116,97,22,6,103,68,111,98,114,111,8,0];

// (a) format freeze: fresh serialize reproduces the golden bytes exactly.
const fresh: any = serialize(buildGraph());
let sameLen = fresh.length === GOLDEN.length;
let firstDiff = -1;
if (sameLen) {
  for (let i = 0; i < GOLDEN.length; i++) {
    if (fresh[i] !== GOLDEN[i]) {
      firstDiff = i;
      break;
    }
  }
}

// (b) cross-process contract: the golden stream (produced by an EARLIER
// process) revives completely here.
const g: any = deserialize(GOLDEN);

// (c) disk round-trip: serialize → file → read → deserialize.
const diskPath = tmpdir() + "\\claude-pickle-golden.rtsp";
writeFileSync(diskPath, serialize(buildGraph()) as any);
const fromDisk: any = deserialize(readFileSync(diskPath) as any);

describe("rts:serde RTSP v1 golden (format freeze + cross-process)", () => {
  test("fresh serialize is byte-identical to golden", () => {
    expect(sameLen).toBe(true);
    expect(firstDiff).toBe(-1);
  });

  test("golden: primitives", () => {
    expect(g.titulo).toBe("golden v1");
    expect(g.n).toBe(3.5);
    expect(g.i).toBe(7);
    expect(g.flag).toBe(true);
    expect(g.nada).toBe(null);
    expect(g.indef).toBe(undefined);
  });

  test("golden: array + shared identity + cycle", () => {
    expect(g.lista.length).toBe(4);
    expect(g.lista[1]).toBe("dois");
    expect(g.lista[3] === g.outra).toBe(true);
    expect(g.outra.id).toBe(42);
    expect(g.ciclo === g).toBe(true);
  });

  test("golden: Date + RegExp + Error", () => {
    expect(g.quando.getTime()).toBe(1700000000000);
    expect(g.padrao.source).toBe("ab+c");
    expect(g.padrao.flags).toBe("gi");
    expect(g.erro.message).toBe("golden boom");
  });

  test("golden: Map + Set revive", () => {
    expect(g.mapa instanceof Map).toBe(true);
    expect(g.mapa.get("b")).toBe(2);
    expect(g.conjunto instanceof Set).toBe(true);
    expect(g.conjunto.has("y")).toBe(true);
  });

  test("golden: class instance + inheritance + fn by reference", () => {
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
