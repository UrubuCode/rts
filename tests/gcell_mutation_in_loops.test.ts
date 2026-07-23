import { describe, test, expect } from "rts:test";

// Mutation of a module-global inside a for / for-of / try / switch must be seen
// (the counters must advance). The mutation scan used to SKIP those statement
// kinds, which could mis-classify a written gcell as immutable → memoized → read
// stale. This is the diagnosed cause of #1978; note the FULL #1978 failure is
// scale-dependent (only reproduces in a huge __rtsn_main) so these minimal cases
// pass with or without the fix — they guard the patterns going forward, they are
// not a minimal reproduction of #1978.

// counter written inside a `for` inside a top-level `while`
let inFor = 0;
let iter = 0;
while (iter < 3) {
    for (let k = 0; k < 4; k++) {
        inFor++;
    }
    iter++;
}

// counter written inside a `for-of`
let inForOf = 0;
const xs = [10, 20, 30];
for (const x of xs) {
    inForOf += x;
}

// counter written inside a `try`
let inTry = 0;
try {
    inTry = 5;
} catch (e) {
    inTry = -1;
}

// counter written inside a `switch`
let inSwitch = 0;
const sel = 2;
switch (sel) {
    case 2:
        inSwitch = 42;
        break;
    default:
        inSwitch = -1;
}

describe("gcell mutation inside loop/try/switch (#1978)", () => {
    test("for-in-while counter advances", () => { expect(inFor).toBe(12); });
    test("for-of accumulator advances", () => { expect(inForOf).toBe(60); });
    test("try assignment is seen", () => { expect(inTry).toBe(5); });
    test("switch assignment is seen", () => { expect(inSwitch).toBe(42); });
});
