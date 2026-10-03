import { describe, test, expect } from "rts:test";
import { Target, Plain, Replaced, trace } from "./_claude_decorator_exported";
import * as mod from "./_claude_decorator_exported";

// A decorated class that is EXPORTED. This was a second defect, independent of
// whether the decorator ran: the desugaring answers a block, and the two
// readers of an exported declaration's names — `emit::module::declared_names`
// and `check::module::declared_names` — did not look inside one, so the module
// published nothing under the class's name. Measured 2026-10-03 against
// bun 1.4.0: `typeof Target` answered `"undefined"` here and `"function"`
// there, while `Plain` beside it exported correctly.

describe("claude-decorator-export", () => {
  test("a decorated class is published under its own name", () => {
    expect(typeof Target).toBe("function");
    expect(new Target().x).toBe(1);
    expect(new Target().hello()).toBe("target");
  });

  test("an undecorated class beside it is unaffected", () => {
    expect(typeof Plain).toBe("function");
    expect(new Plain().y).toBe(2);
  });

  test("every decorator in the module ran, in source order", () => {
    expect(trace.join(",")).toBe(
      "marked:target|function,marked:replaced|function",
    );
  });

  test("the scratch binding the desugaring uses is not exported", () => {
    // The desugaring needs one temporary per decorated member. Holding it as a
    // `var` beside the class binding would have put it in the module scope, and
    // `declared_names` publishes everything an exported declaration binds — so
    // the module would have grown an export nobody wrote. It is a `let` in a
    // block of its own for exactly this reason, and this is the assertion that
    // says so.
    const keys = Object.keys(mod).sort().join(",");
    expect(keys).toBe("Mark,Plain,Replaced,Target,trace");
    expect(new Replaced().z).toBe(3);
  });
});
