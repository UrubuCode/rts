import { describe, test, expect } from "rts:test";
import { serialize, deserialize } from "rts:serde";

// Pre-compute at top-level (instance methods inside test() closures can hit
// GC — handle collected before use).

// 1. primitives + plain object + array round-trip
const plain: any = { name: "root", n: 42, f: 3.5, ok: true, nil: null, list: [1, 2.5, "x", false] };
const r1: any = deserialize(serialize(plain));

// 2. cycle — what JSON cannot express
const cyc: any = { tag: "cyc" };
cyc.self = cyc;
const r2: any = deserialize(serialize(cyc));

// 3. shared identity — two fields, ONE object after the trip
const shared: any = { v: 7 };
const r3: any = deserialize(serialize({ x: shared, y: shared }));
r3.x.v = 99;

// 4. deep nesting + arrays of objects
const deep: any = { rows: [{ id: 1 }, { id: 2 }], meta: { of: { depth: 3 } } };
const r4: any = deserialize(serialize(deep));

// 5. Date + RegExp via extension codecs
const rDate: any = deserialize(serialize({ when: new Date(1700000000000) }));
const rRe: any = deserialize(serialize({ re: /ab+c/gi }));
const reTest = rRe.re.test("xABBC");

// 6. Error round-trips its fields
const rErr: any = deserialize(serialize(new Error("boom")));

// 7. cyclic array
const arr: any[] = [1, 2];
arr.push(arr);
const r7: any = deserialize(serialize(arr));

// 8. serialize output is a byte array (all 0..255)
const bytes: number[] = serialize({ a: 1 }) as any;
let allBytes = true;
for (let i = 0; i < bytes.length; i++) {
  if (bytes[i] < 0 || bytes[i] > 255) allBytes = false;
}

// 9. functions are unserializable — TypeError, like Python's pickle
let fnThrew = false;
try {
  serialize(() => 1);
} catch (e) {
  fnThrew = true;
}

// 10. special numbers survive (NaN via isNaN, ±Infinity, -0)
const nums: any = deserialize(serialize([NaN, Infinity, -Infinity, -0, 1e300]));

// ── phase 2: class instances ──
class PkPessoa {
  nome: string;
  idade: number;
  constructor(n: string, i: number) {
    this.nome = n;
    this.idade = i;
  }
  saudacao(): string {
    return "oi " + this.nome;
  }
}
class PkConta {
  #saldo: number;
  dono: PkPessoa;
  constructor(d: PkPessoa, s: number) {
    this.dono = d;
    this.#saldo = s;
  }
  get saldo(): number {
    return this.#saldo;
  }
  depositar(v: number): void {
    this.#saldo = this.#saldo + v;
  }
}
class PkAnimal {
  nome: string;
  constructor(n: string) {
    this.nome = n;
  }
  som(): string {
    return "...";
  }
}
class PkCao extends PkAnimal {
  raca: string;
  constructor(n: string, r: string) {
    super(n);
    this.raca = r;
  }
  som(): string {
    return "au au";
  }
}
class PkNode {
  val: number;
  next: PkNode | null;
  constructor(v: number) {
    this.val = v;
    this.next = null;
  }
}

const inst: any = deserialize(serialize(new PkPessoa("Ana", 30)));
const conta: any = deserialize(serialize(new PkConta(new PkPessoa("Bia", 20), 100)));
conta.depositar(50);
const dog: any = deserialize(serialize(new PkCao("Rex", "vira-lata")));
const na = new PkNode(1);
const nb = new PkNode(2);
na.next = nb;
nb.next = na;
const cycNode: any = deserialize(serialize(na));

// ── phase 2: Map/Set ──
const srcMap = new Map<string, number>();
srcMap.set("a", 1);
srcMap.set("b", 2);
const rMap: any = deserialize(serialize(srcMap));
rMap.set("c", 3);
const srcSet = new Set<number>();
srcSet.add(10);
srcSet.add(20);
const rSet: any = deserialize(serialize(srcSet));
const keyObj: any = { k: 1 };
const objMap = new Map<any, string>();
objMap.set(keyObj, "valor");
const rObjMap: any = deserialize(serialize({ map: objMap, key: keyObj }));

// ── phase 3: functions by reference ──
function pkDobro(x: number): number {
  return x * 2;
}
const rFn: any = deserialize(serialize(pkDobro));
const rFnObj: any = deserialize(serialize({ handler: pkDobro }));
let arrowThrew = false;
try {
  const k = 7;
  serialize((x: number) => x + k);
} catch (e) {
  arrowThrew = true;
}
let boundThrew = false;
try {
  serialize(pkDobro.bind(null, 5));
} catch (e) {
  boundThrew = true;
}

describe("rts:serde pickle", () => {
  test("plain object + array round-trip", () => {
    expect(r1.name).toBe("root");
    expect(r1.n).toBe(42);
    expect(r1.f).toBe(3.5);
    expect(r1.ok).toBe(true);
    expect(r1.nil).toBe(null);
    expect(r1.list.length).toBe(4);
    expect(r1.list[2]).toBe("x");
  });

  test("cycle survives", () => {
    expect(r2.tag).toBe("cyc");
    expect(r2.self === r2).toBe(true);
  });

  test("shared identity is one object", () => {
    expect(r3.x === r3.y).toBe(true);
    expect(r3.y.v).toBe(99);
  });

  test("deep nesting", () => {
    expect(r4.rows.length).toBe(2);
    expect(r4.rows[1].id).toBe(2);
    expect(r4.meta.of.depth).toBe(3);
  });

  test("Date via ext codec", () => {
    expect(rDate.when.getTime()).toBe(1700000000000);
  });

  test("RegExp via ext codec", () => {
    expect(rRe.re.source).toBe("ab+c");
    expect(rRe.re.flags).toBe("gi");
    expect(reTest).toBe(true);
  });

  test("Error fields round-trip", () => {
    expect(rErr.message).toBe("boom");
    expect(rErr.name).toBe("Error");
  });

  test("cyclic array", () => {
    expect(r7[0]).toBe(1);
    expect(r7[2] === r7).toBe(true);
  });

  test("output is bytes", () => {
    expect(bytes.length > 5).toBe(true);
    expect(allBytes).toBe(true);
  });

  test("function throws TypeError", () => {
    expect(fnThrew).toBe(true);
  });

  test("special numbers", () => {
    expect(isNaN(nums[0])).toBe(true);
    expect(nums[1]).toBe(Infinity);
    expect(nums[2]).toBe(-Infinity);
    expect(nums[4]).toBe(1e300);
  });

  test("class instance: fields + instanceof + method", () => {
    expect(inst.nome).toBe("Ana");
    expect(inst.idade).toBe(30);
    expect(inst instanceof PkPessoa).toBe(true);
    expect(inst.saudacao()).toBe("oi Ana");
  });

  test("class instance: #private + getter + nested instance", () => {
    expect(conta.saldo).toBe(150);
    expect(conta.dono instanceof PkPessoa).toBe(true);
    expect(conta.dono.saudacao()).toBe("oi Bia");
  });

  test("class inheritance: instanceof chain + override", () => {
    expect(dog instanceof PkCao).toBe(true);
    expect(dog instanceof PkAnimal).toBe(true);
    expect(dog.nome).toBe("Rex");
    expect(dog.som()).toBe("au au");
  });

  test("cycle through class instances", () => {
    expect(cycNode.val).toBe(1);
    expect(cycNode.next.val).toBe(2);
    expect(cycNode.next.next === cycNode).toBe(true);
    expect(cycNode.next instanceof PkNode).toBe(true);
  });

  test("Map round-trip + mutation after revive", () => {
    expect(rMap instanceof Map).toBe(true);
    expect(rMap.size).toBe(3);
    expect(rMap.get("a")).toBe(1);
    expect(rMap.get("c")).toBe(3);
  });

  test("Set round-trip", () => {
    expect(rSet instanceof Set).toBe(true);
    expect(rSet.size).toBe(2);
    expect(rSet.has(10)).toBe(true);
    expect(rSet.has(99)).toBe(false);
  });

  test("Map with object key keeps identity", () => {
    expect(rObjMap.map.get(rObjMap.key)).toBe("valor");
  });

  test("function by reference", () => {
    expect(rFn(21)).toBe(42);
    expect(rFnObj.handler(5)).toBe(10);
  });

  test("arrow/bound functions throw", () => {
    expect(arrowThrew).toBe(true);
    expect(boundThrew).toBe(true);
  });
});
