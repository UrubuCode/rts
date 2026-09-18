import { describe, test, expect } from "rts:test";
import { serialize, deserialize } from "rts:serde";

// A Map or Set keys on SameValueZero, and a bigint compares BY VALUE under
// it: `new Set([1n]).has(1n)` is true although the two `1n`s are two heap
// cells. It was false here — the table compared non-numbers by slot and then
// by text, and a bigint is neither — so a Set of ids never found one, and
// `size` counted every insertion. Pre-computed at top level, as the pickle
// tests are.

const big = 2n ** 70n;
const set = new Set<bigint>();
set.add(1n);
set.add(1n);
set.add(big);
set.add(2n ** 70n);
set.add(0n);
set.add(-0n);

const hasOne = set.has(1n);
const hasBig = set.has(2n ** 70n);
const hasZeroViaNegative = set.has(-0n);
const sizeBefore = set.size;
const deleted = set.delete(1n);
const sizeAfter = set.size;
const hasOneAfter = set.has(1n);

const map = new Map<any, string>();
map.set(1n, "bigint one");
map.set(1, "number one");
map.set(big, "big");
const getOne = map.get(1n);
const getNumberOne = map.get(1);
const getBig = map.get(2n ** 70n);
const mapSize = map.size;

// Many bigint keys, so the lookup goes through the hash index and not the
// scan an unindexed table answers from.
const many = new Map<bigint, number>();
for (let i = 0; i < 100; i++) {
  many.set(BigInt(i) * 1000000007n, i);
}
const foundLate = many.get(99n * 1000000007n);
const notFound = many.get(1n);

// The pickle revives the same keys, and the revived table finds them.
const revived: any = deserialize(serialize({ set, map }));
const revivedHas = revived.set.has(2n ** 70n);
const revivedGet = revived.map.get(1n);
const revivedNumber = revived.map.get(1);

describe("Map and Set keyed by a bigint", () => {
  test("a Set finds a bigint by value", () => {
    expect(hasOne).toBe(true);
    expect(hasBig).toBe(true);
    expect(hasZeroViaNegative).toBe(true);
  });

  test("size counts values, not insertions", () => {
    expect(sizeBefore).toBe(3);
    expect(deleted).toBe(true);
    expect(sizeAfter).toBe(2);
    expect(hasOneAfter).toBe(false);
  });

  test("a number and a bigint of the same magnitude are two keys", () => {
    expect(getOne).toBe("bigint one");
    expect(getNumberOne).toBe("number one");
    expect(getBig).toBe("big");
    expect(mapSize).toBe(3);
  });

  test("the hash index finds a bigint among a hundred", () => {
    expect(foundLate).toBe(99);
    expect(notFound).toBe(undefined);
  });

  test("a pickled collection keyed by bigints revives and answers", () => {
    expect(revivedHas).toBe(true);
    expect(revivedGet).toBe("bigint one");
    expect(revivedNumber).toBe("number one");
  });
});
