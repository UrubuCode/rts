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
//
// The mechanism, located 2026-09-20: a captured local is a PROPERTY of an
// environment object, and `Binding::InEnvironment { hops, name }`
// (`emit/scope.rs`) keys the slot by that name. `emit/binding.rs`'s declaration
// path asks `scope.is_captured(name)` and, when it holds, stores into the
// environment property of that name. The captured SET is keyed by spelling too,
// so the loop's own `let i` -- a different binding that merely spells the same
// thing -- is taken for the captured one and writes the outer binding's slot.
// The closure then reads what the loop wrote.
//
// It is not fixable by a narrower test at that site: whether a block-scoped
// declaration needs environment storage of its own depends on whether an inner
// closure captures IT (`catch (c) { const read = () => c; }` is the shape that
// does, and `Scope::enter_environment` is what serves it). Deciding that needs
// binding identity, which is E2. A slot keyed by identity instead of by spelling
// makes the two `i`s two slots and the question disappears.
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
