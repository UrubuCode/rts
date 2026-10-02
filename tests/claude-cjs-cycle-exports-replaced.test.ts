import { describe, test, expect } from "rts:test";

// A CommonJS cycle where BOTH members replace `module.exports` with a function
// on their first line — the shape `protobufjs/src/*.js` is written in, and the
// one that made `require("protobufjs")` fail with "Object prototype may only be
// an Object or null".
//
// What this pins is that the second ask inside a cycle answers the in-progress
// module's CURRENT `module.exports`, not the object it was given at entry. The
// values below were measured on Node 22 (2026-10-02):
//
//   node -e "const A=require('./claude-cjs-cycle-a.js'); …"
//   function function 0
//   function object true A
//
// `b.js` runs while `a.js` is suspended at its own `require`, and it reads
// `A.prototype` — so an answer of the entry object (which has no `prototype`)
// is an `Object.create(undefined)` and a TypeError, which is exactly what the
// engine did.
const A: any = require("./claude-cjs-cycle-a.js");
const B: any = A.sawFromB;

describe("a CommonJS cycle answers the in-progress module's current exports", () => {
  test("the entry module's own exports is the function it assigned", () => {
    expect(typeof A).toBe("function");
  });

  test("the module inside the cycle saw a function, not the entry object", () => {
    expect(B.sawA).toBe("function");
  });

  test("and could therefore reach its prototype", () => {
    expect(B.sawProto).toBe("object");
  });

  test("the prototype it derived from is the same object", () => {
    expect(Object.getPrototypeOf(B.derived) === A.prototype).toBe(true);
  });

  test("so a property added to that prototype afterwards is inherited", () => {
    expect(B.derived.tag).toBe("A");
  });
});
