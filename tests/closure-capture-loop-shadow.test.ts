// A closure reads the binding its BODY was written against. A `for (let i …)`
// in the enclosing function declares a second `i`, per iteration, and the
// closure must not see any of them.
//
// **THIS FILE FAILS TODAY.** It is pinned rather than fixed because the fix is
// not local: `Name` is an interned spelling (`names/mod.rs:23`) and no stage
// turns one into a binding, so a closure's free name and a loop target that
// spell the same thing are one name to every consumer. `docs/engine/
// four-stages.md` E2 is that stage; this is the assertion that will say it
// landed.
//
// It is NOT the inliner, and the ablation is what says so. Measured 2026-09-20:
// adding `const hold = q` -- which makes the helper read as a value, so
// `omit::omittable` refuses it and no call is substituted -- leaves the answer
// unchanged at `0:0 1:1 2:2`. A defect that survives its suspected cause being
// disabled has another cause.
//
// That distinguishes it from `tests/inline-free-name-block-shadow.test.ts`,
// where the same ablation answers correctly and the substitution was the cause.
import { describe, test, expect } from "rts:test";

describe("a loop target that spells a captured name", () => {
  test("a closure reads the binding it was written against", () => {
    function held(): string {
      let i = 7;
      const q = (x: number) => x + i;
      let s = "";
      for (let i = 0; i < 3; i++) {
        s += q(0) + ":" + i + " ";
      }
      return s.trim();
    }
    expect(held()).toBe("7:0 7:1 7:2");
  });

  // The same, with the helper read as a value so no call can be substituted.
  // Both assertions must hold; today both fail, with the same answer.
  test("and still does when nothing may be substituted", () => {
    function held(): string {
      let i = 7;
      const q = (x: number) => x + i;
      const hold = q;
      let s = "";
      for (let i = 0; i < 3; i++) {
        s += q(0) + ":" + i + " ";
      }
      return s.trim() + " " + (hold === q);
    }
    expect(held()).toBe("7:0 7:1 7:2 true");
  });
});
