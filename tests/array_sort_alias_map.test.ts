import { describe, test, expect } from "rts:test";

// Regression (cross-runtime): `const s = arr.sort(cmp); s.map(...)` crashava
// SIGILL. Causa: collect_array_receiver_idents nao reconhecia sort/reverse/
// fill/copyWithin como array-returning, entao a var alias `s` nao era
// registrada em ARRAY_RECEIVER_IDENTS — `s.map(...)` nao era reconhecido como
// array method e caia em rota errada -> crash. Fix: adicionar os mutators
// in-place (que retornam o proprio array/this) a lista array-returning.

let out = "";
function print(v: string): void { out += v + "\n"; }

// sort + alias + map
const items = [{ n: "c", p: 3 }, { n: "a", p: 1 }, { n: "b", p: 2 }];
const s = items.sort((x, y) => x.p - y.p);
print(s.map(i => i.n).join(""));        // abc

// reverse + alias + map
const arr = [1, 2, 3];
const r = arr.reverse();
print(r.map(x => x).join(""));          // 321

// sort chain direto (sem alias)
const nums = [{ v: 9 }, { v: 1 }, { v: 5 }];
print(nums.sort((a, b) => a.v - b.v).map(o => o.v).join(",")); // 1,5,9

// fill + alias
const f = [0, 0, 0].fill(7);
print(f.map(x => x).join(","));         // 7,7,7

describe("array sort alias map", () => {
  test("mutators registram var alias como array", () =>
    expect(out).toBe("abc\n321\n1,5,9\n7,7,7\n"));
});
