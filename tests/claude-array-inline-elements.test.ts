// An array small enough keeps its elements in its own cell, and none of the
// ways that can go wrong is visible as a crash.
//
// Every assertion here failed silently at some point while the storage was
// written: the elements read as `undefined`, the process exited zero, and all
// 375 unit tests of the crate stayed green. They are pinned by the ANSWER
// rather than by the storage on purpose — a test that asserted "these live in
// the cell" would pass just as well if the cell were never used.

import { expect, test } from "rts:test";

test("elements read back from a cell-held array", () => {
  const a = [1, 2, 3];
  expect(a[0]).toBe(1);
  expect(a[2]).toBe(3);
  expect(a.length).toBe(3);
});

test("a property added to an array does not land on an element", () => {
  // This is the one that broke. A property is given a slot counted from zero
  // and written straight into the cell, so a new property takes the slot an
  // element occupies unless the elements move out first. The move has to
  // happen when the cell is RETYPED, because the inline-cache write path takes
  // its own shape transition and never passes through `objects::put`.
  const b = [10, 20, 30];
  (b as Record<string, unknown>).foo = 99;
  expect(b[0]).toBe(10);
  expect(b[1]).toBe(20);
  expect(b[2]).toBe(30);
  expect((b as Record<string, unknown>).foo).toBe(99);
  expect(b.length).toBe(3);
});

test("a frozen array keeps its elements when a property write is refused", () => {
  // The refusal happens before the move, so an array that declines the
  // property must not have moved its elements for nothing — and must still
  // answer them either way.
  const c = [4, 5, 6];
  Object.freeze(c);
  try {
    (c as Record<string, unknown>).nope = 1;
  } catch {
    // Strict mode throws; sloppy mode is a silent no-op. Either is fine here.
  }
  expect(c[0]).toBe(4);
  expect(c[2]).toBe(6);
  expect(c.length).toBe(3);
});

test("a mutation moves the elements out and loses none of them", () => {
  const d = [7, 8];
  d.push(9);
  expect(d.length).toBe(3);
  expect(d[0]).toBe(7);
  expect(d[2]).toBe(9);
  d[0] = 70;
  expect(d[0]).toBe(70);
  d.pop();
  expect(d.length).toBe(2);
  expect(d[1]).toBe(8);
});

test("an array past the inline capacity still answers every element", () => {
  // Fourteen fit; this has seventeen, so it is built in the side table from
  // the start. The boundary is what a one-off constant gets wrong.
  const wide: number[] = [];
  for (let i = 0; i < 17; i++) wide.push(i);
  expect(wide.length).toBe(17);
  expect(wide[0]).toBe(0);
  expect(wide[16]).toBe(16);

  const exactly14 = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
  expect(exactly14.length).toBe(14);
  expect(exactly14[13]).toBe(13);

  const exactly15 = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
  expect(exactly15.length).toBe(15);
  expect(exactly15[14]).toBe(14);
});

test("references in a cell-held array survive a collection", () => {
  // The reason this is safe is that `gc::traces_field` follows a slot the
  // layout does not declare, so the element slots are traced without anything
  // being told about them. If that direction ever flips, this is the test that
  // fails — and it would otherwise be a use-after-free rather than a wrong
  // answer.
  const kept = [{ v: 1 }, { v: 2 }];
  for (let i = 0; i < 200000; i++) {
    const garbage = [i, i + 1, i + 2];
    if (garbage[0] < 0) throw new Error("unreachable");
  }
  expect(kept[0].v).toBe(1);
  expect(kept[1].v).toBe(2);
});

test("a hole in a cell-held array stays absent", () => {
  const f = [1, , 3];
  expect(f.length).toBe(3);
  expect(1 in f).toBe(false);
  expect(0 in f).toBe(true);
  expect(f[2]).toBe(3);
});

test("length changes reconcile with the elements", () => {
  const g = [1, 2, 3, 4];
  g.length = 2;
  expect(g.length).toBe(2);
  expect(g[0]).toBe(1);
  expect(g[2]).toBe(undefined);
  g.length = 4;
  expect(g.length).toBe(4);
  expect(3 in g).toBe(false);
});
