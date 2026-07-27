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
});
