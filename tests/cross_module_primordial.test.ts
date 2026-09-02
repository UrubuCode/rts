// `primordial::untouched` was computed with ONE unit's body, at both doors, so
// in a graph compilation it answered a question about one file:
//
//   a.ts     export function go() { let v = 16.0; return Math.sqrt(v); }
//   main.ts  import { go } from "./a";
//            Math.sqrt = () => 42;
//            console.log(go());        // 4 here, 42 in node, exit code 0
//
// `go` is emitted while unit A is being emitted, and A leaves `Math` alone, so
// `Math.sqrt(v)` lowered to a machine instruction no assignment can reach —
// while the module calling it had replaced the function.
//
// `primordial.rs`'s own header requires this proof to be whole-program: it says
// the answer is available "because the whole tree is compiled before anything
// runs", and records that the 2026-08-05 rule was reversed "on the condition
// that the proof be whole-program". It was not, in a graph.
//
// `emit_modules` now lowers every unit's exports BEFORE the emit loop and folds
// `untouched` over all of them. This file is single-unit and therefore cannot
// reach the graph door; what it pins is the half a single file CAN observe —
// that the fold still fires where nothing writes, and stops where something
// does. The cross-module halves are in `crates/rts-host/tests/running.rs`,
// which can compile a graph.
import { describe, test, expect } from "rts:test";

describe("the proof the whole program has to answer", () => {
  test("an undisturbed Math is still an instruction", () => {
    let v = 16.0;
    expect(Math.sqrt(v)).toBe(4);
    let w = 3.7;
    expect(Math.floor(w)).toBe(3);
  });

  test("a written Math is not", () => {
    const held = Math.sqrt;
    try {
      Math.sqrt = () => 42;
      let v = 16.0;
      expect(Math.sqrt(v)).toBe(42);
    } finally {
      Math.sqrt = held;
    }
  });

  test("the write may come after the function that reads it", () => {
    // The proof is over the whole body, not over what precedes the call, so
    // where the write is written does not matter.
    function reads(): number {
      let v = 16.0;
      return Math.sqrt(v);
    }
    const held = Math.sqrt;
    try {
      Math.sqrt = () => 42;
      expect(reads()).toBe(42);
    } finally {
      Math.sqrt = held;
    }
  });
});
