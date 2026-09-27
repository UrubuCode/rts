import { describe, test, expect } from "rts:test";

// A `const` arrow at the top of a module that is only ever called is still a real
// function: the module's own code is not compiled by the MIR stage, so nothing
// substitutes its calls. Compiled as if it were a block of the module, its read of a
// module binding was taken for a read before that binding's declaration.

let v = "outer";
const readOuter = () => v;
let readInner: () => string = () => "unset";
{
  let v = "inner";
  readInner = () => v;
  v = "inner2";
}
const outerNow = readOuter();
v = "changed";

describe("an arrow declared at module scope", () => {
  test("reads the module's binding, live", () => {
    expect(outerNow).toBe("outer");
    expect(readOuter()).toBe("changed");
  });
  test("a block's shadowing binding is the block's", () => {
    expect(readInner()).toBe("inner2");
  });
});
