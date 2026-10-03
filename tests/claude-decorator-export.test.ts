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

// A namespace publishes its exported members onto an object, and asks the same
// question through a THIRD reader — `parse::item::names_bound_by`. It had the
// same answer: nothing. `new N.C()` answered `TypeError: undefined is not a
// constructor` while `C` itself had been built and decorated correctly.
const nsLog: string[] = [];
function Nested(name: string) {
  return (target: any) => {
    nsLog.push(name + ":" + typeof target);
  };
}
namespace Container {
  @Nested("Inner")
  export class Inner {
    value = 9;
  }
  export class Bare {
    value = 8;
  }
}

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

  test("a namespace publishes a decorated class it exports", () => {
    expect(typeof Container.Inner).toBe("function");
    expect(new Container.Inner().value).toBe(9);
    expect(new Container.Bare().value).toBe(8);
    expect(nsLog.join(",")).toBe("Inner:function");
  });
});
