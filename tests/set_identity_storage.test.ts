import { describe, test, expect } from "rts:test";

// (#394) Set storage preserva a IDENTIDADE do elemento.
//
// Antes o Set armazenava `1` como value interno e a key de objetos virava
// vazia (STRING_PTR de nao-string = ptr nulo), entao todos os objetos
// colidiam numa unica entrada e a leitura (values/spread/for-of) re-parseava
// a key perdendo o handle. Agora:
// - a KEY estavel: conteudo p/ string, "\0obj#<h>" p/ objeto/Set (identidade),
//   decimal p/ number;
// - o VALUE eh o elemento original -> leitura recupera a identidade.

// --- Set de Sets aninhados via add() + for-of aninhado ---
const inner1 = new Set<number>([1, 2]);
const inner2 = new Set<number>([3, 4]);
const outer = new Set<Set<number>>();
outer.add(inner1);
outer.add(inner2);
let nestedTotal = 0;
for (const s of outer) {
  for (const n of s) {
    nestedTotal += n;
  }
}

// --- Set de Sets via construtor literal ---
const a = new Set<number>([10, 20]);
const b = new Set<number>([30]);
const lit = new Set<Set<number>>([a, b]);
let litTotal = 0;
for (const s of lit) {
  for (const n of s) {
    litTotal += n;
  }
}
const litSize = lit.size;

// --- has/delete por identidade de objeto ---
const obj1 = new Set<number>([1]);
const obj2 = new Set<number>([2]);
const holder = new Set<Set<number>>();
holder.add(obj1);
const hasObj1 = holder.has(obj1);
const hasObj2 = holder.has(obj2); // identidade distinta -> false
holder.delete(obj1);
const afterDelete = holder.size;

// --- primitivos continuam corretos (nao-regressao) ---
const nums = new Set<number>();
nums.add(5);
nums.add(7);
nums.add(5); // dedup
const numsArr = [...nums].join(",");
const numsHas = nums.has(7);
const numsHasMissing = nums.has(99);

const strs = new Set<string>(["x", "y", "x"]);
const strsArr = Array.from(strs.values()).join(",");
const strsHas = strs.has("y");

// --- Map nao-regrediu (has/delete por key-string) ---
const m = new Map<string, number>();
m.set("k", 42);
const mapHas = m.has("k");
const mapGet = m.get("k");
m.delete("k");
const mapAfter = m.has("k");

describe("Set storage preserva identidade (#394)", () => {
  test("Sets aninhados via add + for-of", () => expect(`${nestedTotal}`).toBe("10"));
  test("Sets aninhados via literal", () => expect(`${litTotal}`).toBe("60"));
  test("size do Set de Sets literal", () => expect(`${litSize}`).toBe("2"));
  test("has por identidade (mesmo obj)", () => expect(hasObj1).toBe(true));
  test("has por identidade (obj distinto)", () => expect(hasObj2).toBe(false));
  test("delete por identidade", () => expect(`${afterDelete}`).toBe("0"));

  test("Set numerico spread", () => expect(numsArr).toBe("5,7"));
  test("Set numerico has", () => expect(numsHas).toBe(true));
  test("Set numerico has ausente", () => expect(numsHasMissing).toBe(false));
  test("Set string values", () => expect(strsArr).toBe("x,y"));
  test("Set string has", () => expect(strsHas).toBe(true));

  test("Map has nao-regrediu", () => expect(mapHas).toBe(true));
  test("Map get nao-regrediu", () => expect(`${mapGet}`).toBe("42"));
  test("Map delete nao-regrediu", () => expect(mapAfter).toBe(false));
});
