import { describe, test, expect } from "rts:test";

// Leaving a `for`-`of` EARLY, and what the loop owes on the way out.
//
// A `for`-`of` here is one loop with two arms: an indexed walk for an array or a
// string, the stepped protocol for everything else, and the array-walked arm has
// no iterator object at all — `emit/foreach.rs` binds its `__rts_of_it` to
// `undefined` and says so. The close run on the `return` and on the throw paths
// was UNCONDITIONAL, so every array loop that left early read `.return` off
// `undefined` and raised `TypeError: Cannot read properties of undefined
// (reading 'return')` instead of leaving.
//
// Every number below was measured in bun 1.4 and in node 24, which agree. The
// `closed` counters are the point of the stepped half: an answer alone cannot
// tell a `return()` that was owed from one that was not, and both directions are
// wrong — a missing close leaks, an extra one is an observable call on a user's
// iterator after `done`.

// Each of these reads `expect` — a LIVE IMPORT — before it loops, and that line
// is the fixture's own gate rather than a guard the behaviour needs. The MIR
// stage declines "a body that reads a live import or an exported binding"
// (`emit/through_mir.rs`), so this is what keeps these bodies on the `emit/`
// path, which is the path the defect lives on: without it the small ones are
// compiled by `lower/` and pass either way. The throw case below needs no such
// line — it is written inline in a test body, which is already declined.
function earlyOut(xs: number[]): number {
  if (typeof expect !== "function") return 0;
  for (const x of xs) { if (x > 1) return x; }
  return -1;
}

function earlyOutLiteral(): number {
  if (typeof expect !== "function") return 0;
  for (const x of [1, 2, 3]) { if (x > 1) return x; }
  return -1;
}

function earlyOutNested(): number {
  if (typeof expect !== "function") return 0;
  for (const a of [1, 2]) { for (const b of [3, 4]) { if (b === 4) return a * 10 + b; } }
  return -1;
}

function earlyOutDestructured(): number {
  if (typeof expect !== "function") return 0;
  for (const [k, v] of [["a", 1], ["b", 2]] as [string, number][]) { if (k === "b") return v; }
  return -1;
}

function earlyOutText(): string {
  if (typeof expect !== "function") return "?";
  for (const ch of "abc") { if (ch === "b") return ch; }
  return "-";
}

function closable(values: number[]) {
  let closed = 0;
  const iterable: any = {
    [Symbol.iterator]() {
      let i = 0;
      return {
        next: () => (i < values.length ? { value: values[i++], done: false } : { value: undefined, done: true }),
        return() { closed++; return { done: true }; },
      };
    },
  };
  return { iterable, count: () => closed };
}

describe("for-of, leaving early", () => {
  test("a return out of the body of an array loop answers rather than throwing", () => {
    expect(earlyOut([1, 2, 3])).toBe(2);
    expect(earlyOutLiteral()).toBe(2);
    expect(earlyOutNested()).toBe(14);
    expect(earlyOutDestructured()).toBe(2);
    expect(earlyOutText()).toBe("b");
  });

  test("a throw out of the body of an array loop reaches the catch", () => {
    let hit = "";
    try {
      for (const x of [1, 2, 3]) { if (x === 2) throw new Error("boom"); }
    } catch (e: any) {
      hit = e.message;
    }
    expect(hit).toBe("boom");
  });

  test("a labelled continue past an array loop still leaves it", () => {
    let seen = 0;
    outer: for (const a of [1, 2]) {
      for (const b of [3, 4]) { seen++; if (b === 3) continue outer; }
    }
    expect(seen).toBe(2);
  });

  test("a stepped iterator is closed exactly once on return, break and throw", () => {
    const onReturn = closable([1, 2, 3]);
    const got = ((): number => {
      for (const v of onReturn.iterable) { if (v === 2) return v; }
      return -1;
    })();
    expect(got).toBe(2);
    expect(onReturn.count()).toBe(1);

    const onBreak = closable([1, 2, 3]);
    for (const v of onBreak.iterable) { if (v === 2) break; }
    expect(onBreak.count()).toBe(1);

    const onThrow = closable([1, 2, 3]);
    try {
      for (const v of onThrow.iterable) { if (v === 2) throw new Error("x"); }
    } catch (e) {
      // swallowed on purpose: what is asserted is the close, not the error
    }
    expect(onThrow.count()).toBe(1);
  });

  test("an exhausted iterator is NOT closed", () => {
    const drained = closable([1, 2, 3]);
    let sum = 0;
    for (const v of drained.iterable) sum += v;
    expect(sum).toBe(6);
    expect(drained.count()).toBe(0);
  });

  test("an iterator with no return at all is left alone", () => {
    const bare = (): number => {
      const it: any = { [Symbol.iterator]() { let i = 0; return { next: () => ({ value: i++, done: i > 3 }) }; } };
      for (const v of it) { if (v === 1) return v; }
      return -1;
    };
    expect(bare()).toBe(1);
  });

  test("the close runs before a finally the return passes through", () => {
    const order: string[] = [];
    let closed = 0;
    const iterable: any = {
      [Symbol.iterator]() {
        let i = 0;
        return {
          next: () => ({ value: i++, done: i > 3 }),
          return() { order.push("close"); closed++; return { done: true }; },
        };
      },
    };
    const got = ((): number => {
      try {
        for (const v of iterable) { if (v === 1) return v; }
      } finally {
        order.push("finally");
      }
      return -1;
    })();
    expect(got).toBe(1);
    expect(order.join()).toBe("close,finally");
    expect(closed).toBe(1);
  });
});
