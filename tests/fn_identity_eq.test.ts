import { describe, test, expect } from "rts:test";

// Function VALUE identity for `===` / `Object.is` (feat/fn-identity-eq).
// A function value re-reifies a fresh handle on each reference, so `f === f`
// used to be FALSE. This verifies the stable-identity fix.

function plain(): number { return 1; }
const alias = plain;

function makeCounter(): () => number {
    let n = 0;
    return () => ++n;
}
const c1 = makeCounter();
const c2 = makeCounter();

const arrow = (x: number) => x + 1;
const arrowAlias = arrow;

// Pre-compute at top-level (harness/GC safety).
const samePlain = plain === plain;
const aliasEq = plain === alias;
const objIsPlain = Object.is(plain, plain);
const sameArrow = arrow === arrowAlias;
const closureSelf = c1 === c1;
const closuresDiffer = c1 === c2;      // different closures → NOT equal
const plainVsArrow = (plain as unknown) === (arrow as unknown);

// Set<fn> dedup relies on identity.
const s = new Set<() => number>();
s.add(c1);
s.add(c1);
s.add(c2);
const setSize = s.size;                // 2 (c1 once, c2 once)

describe("function value identity", () => {
    test("f === f is true", () => { expect(samePlain).toBe(true); });
    test("aliased fn is equal", () => { expect(aliasEq).toBe(true); });
    test("Object.is(f, f) is true", () => { expect(objIsPlain).toBe(true); });
    test("arrow alias is equal", () => { expect(sameArrow).toBe(true); });
    test("closure === itself", () => { expect(closureSelf).toBe(true); });
    test("distinct closures differ", () => { expect(closuresDiffer).toBe(false); });
    test("different fns differ", () => { expect(plainVsArrow).toBe(false); });
    test("Set<fn> dedups by identity", () => { expect(setSize).toBe(2); });
});
