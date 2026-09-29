import { describe, test, expect } from "rts:test";

// The storage of an array that died is handed to an array made later. What
// this pins is that a program cannot tell: an array that is still reachable
// keeps its own elements through any number of collections, two live arrays
// never share storage, and an array made on recycled storage starts with
// exactly what it was written with — its length included — and grows as any
// other does.

describe("arrays made on storage another array left behind", () => {
  test("a kept array holds what it was given while thousands die around it", () => {
    const kept: number[][] = [];
    let sum = 0;
    for (let i = 0; i < 300_000; i++) {
      const a = [i, i + 1, i + 2];
      if (i % 10_000 === 0) kept.push(a);
      sum += a[0] + a.length;
    }
    expect(sum).toBe((299_999 * 300_000) / 2 + 900_000);
    expect(kept.length).toBe(30);
    expect(kept.every((a, at) => a.length === 3 && a[0] === at * 10_000 && a[2] === at * 10_000 + 2)).toBe(true);
  });
  test("two live arrays never answer for each other", () => {
    for (let round = 0; round < 50_000; round++) { const gone = [round, round]; gone[0] = -1; }
    const a = [1, 2];
    const b = [3, 4];
    a[0] = 10;
    b.push(5);
    expect(a.join()).toBe("10,2");
    expect(b.join()).toBe("3,4,5");
    a.length = 0;
    expect(b.join()).toBe("3,4,5");
    expect(a.length).toBe(0);
  });
  test("it starts with exactly what was written, whatever the last one held", () => {
    for (let round = 0; round < 80_000; round++) { const wide = [1, 2, 3, 4, 5, 6, 7, 8]; wide.push(round); }
    const one = [7];
    expect(one.length).toBe(1);
    expect(one[1]).toBe(undefined);
    expect(1 in one).toBe(false);
    expect(JSON.stringify(one)).toBe("[7]");
    const holes = [1, , 3];
    expect(holes.length).toBe(3);
    expect(1 in holes).toBe(false);
    const mixed: unknown[] = ["s", { k: 1 }, null];
    expect((mixed[1] as any).k).toBe(1);
  });
  test("it grows, shrinks and is copied as any array", () => {
    for (let round = 0; round < 80_000; round++) { const gone = [round]; gone.pop(); }
    const a = [1, 2];
    for (let i = 0; i < 100; i++) a.push(i);
    expect(a.length).toBe(102);
    expect(a[101]).toBe(99);
    const copy = a.slice(0, 3);
    a[0] = -1;
    expect(copy.join()).toBe("1,2,0");
    const nested = [[1, 2], [3, 4]];
    for (let round = 0; round < 80_000; round++) { const gone = [round, round]; gone.reverse(); }
    expect(nested.map((inner) => inner.join("+")).join()).toBe("1+2,3+4");
  });
});
