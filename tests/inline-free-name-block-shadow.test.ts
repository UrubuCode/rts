// A substituted body resolves its FREE names in the caller's scope, so a block
// of the caller that redeclares one of them changes what the body reads.
//
// `emit/inline.rs` has two proofs for a free name. The count -- every free name
// declared exactly once in the whole program -- is sound. `ctx.omits` is the
// other, and it was documented as "stronger rather than weaker": the helper is
// declared HERE, is never read as a value, and is not captured, so every call is
// in the declaring function and the caller IS the declarer.
//
// That establishes no other FUNCTION is between declaration and call site. It
// does not establish that no other BLOCK is, and a block is enough. Measured
// 2026-09-20 against target/release/rts.exe, the first case answered
// `11,110,100` where node answers `11,11,100` -- a silent wrong answer with a
// successful exit.
//
// The assertions are about VALUES, never about whether a call was substituted: a
// refusal is always correct and always slower, so asserting the substitution
// happened would fail for the safe reason.
import { describe, test, expect } from "rts:test";

describe("a free name a block of the caller redeclares", () => {
  test("a read resolves against the declaring scope, not the calling block", () => {
    function held(): string {
      let i = 1;
      const q = (x: number) => x + i;
      let s = "" + q(10);
      {
        let i = 100;
        s += "," + q(10);
        s += "," + i;
      }
      return s;
    }
    expect(held()).toBe("11,11,100");
  });

  // The loop-target form of this shape is NOT here, and that is a finding
  // rather than an omission: it answers wrongly with the substitution disabled
  // as well, so it is not this pass. See
  // `tests/closure-capture-loop-shadow.test.ts`.

  // A WRITE through a free name lands on the declaring binding too. The block's
  // own binding must be untouched by it, and the declarer's must carry the
  // accumulation out.
  test("a written free name accumulates on the declaring binding", () => {
    function held(): string {
      let seen = 0;
      const bump = (n: number) => {
        seen = seen + n;
        return seen;
      };
      const first = bump(2);
      let inner = -1;
      {
        let seen = 500;
        bump(3);
        inner = seen;
      }
      return first + "," + seen + "," + inner;
    }
    expect(held()).toBe("2,5,500");
  });

  // The case that must KEEP working, and the reason the fix counts declarations
  // in the declaring body rather than refusing outright: a free name nothing
  // else in that body declares is substitutable, and refusing it measured 233.67
  // against 46.33 ns on 2026-08-30.
  test("a free name declared once in the declaring body still reads it", () => {
    function held(): number {
      let zwq = 5;
      const q = (x: number) => x + zwq;
      let total = 0;
      for (let at = 0; at < 3; at++) {
        total = total + q(at);
      }
      return total;
    }
    expect(held()).toBe(18);
  });

  // A body-local name the caller also declares. The local is bound by the
  // substitution's own scope layer, so this passes today; it is pinned because
  // the guard that protects it at the other door is absent at this one, and
  // nothing else states the expected answer.
  test("a body local does not write the caller's binding of that name", () => {
    function held(): string {
      const g = (n: number) => {
        const t = n * 2;
        return t + 1;
      };
      let s = "" + g(3);
      {
        let t = 9;
        s += "," + g(4) + "," + t;
      }
      return s;
    }
    expect(held()).toBe("7,9,9");
  });
});
