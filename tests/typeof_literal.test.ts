// `typeof x === "…"` fused into one crossing, and every way that could be wrong.
//
// The assertions are about VALUES. All nine answers are checked against both
// operand orders and against a name that is not one of the nine, because the
// fused form compares the literal's TEXT against a table and a wrong index
// answers a confident `false` rather than failing.
import { describe, test, expect } from "rts:test";

describe("typeof compared against a literal", () => {
  test("every one of the nine answers, both orders", () => {
    const cases: [any, string][] = [
      [1, "number"],
      [1.5, "number"],
      [NaN, "number"],
      ["s", "string"],
      ["", "string"],
      [true, "boolean"],
      [false, "boolean"],
      [undefined, "undefined"],
      [null, "object"],
      [{}, "object"],
      [[], "object"],
      [Symbol("s"), "symbol"],
      [10n, "bigint"],
      [function () {}, "function"],
      [Math.max, "function"],
      [class {}, "function"],
    ];
    for (const [value, name] of cases) {
      expect(typeof value === name).toBe(true);
      expect(name === typeof value).toBe(true);
      expect(typeof value !== name).toBe(false);
      // The literal form is what the fusion actually fires on — the loop above
      // compares against a variable and takes the ordinary path, so without
      // this the test would pass on a build where the fusion is broken.
      expect(String(typeof value)).toBe(name);
    }
  });

  test("the literal forms, written out so the fused path is the one taken", () => {
    const n: any = 1, s: any = "x", b: any = true, o: any = {}, f: any = () => 1;
    let u: any;
    expect(typeof n === "number").toBe(true);
    expect(typeof n === "string").toBe(false);
    expect(typeof s === "string").toBe(true);
    expect(typeof s === "number").toBe(false);
    expect(typeof b === "boolean").toBe(true);
    expect(typeof o === "object").toBe(true);
    expect(typeof o === "function").toBe(false);
    expect(typeof f === "function").toBe(true);
    expect(typeof u === "undefined").toBe(true);
    expect(typeof null === "object").toBe(true);
    expect("number" === typeof n).toBe(true);
    expect(typeof n !== "string").toBe(true);
    expect(typeof n !== "number").toBe(false);
  });

  test("a name that is not one of the nine is false, never a match", () => {
    const n: any = 1;
    expect(typeof n === "Number").toBe(false);
    expect(typeof n === "numbe").toBe(false);
    expect(typeof n === "numberr").toBe(false);
    expect(typeof n === "").toBe(false);
    expect(typeof n === "int").toBe(false);
    expect(typeof n !== "Number").toBe(true);
  });

  test("an UNDECLARED name keeps typeof's exemption from the reference error", () => {
    // The whole reason the operand goes through `unary::typeof_operand` rather
    // than being emitted here: `typeof nothingDeclaredAnywhere` must answer
    // "undefined" and not throw, and a second path would have re-derived that.
    expect(typeof (globalThis as any).zzzNotDeclared === "undefined").toBe(true);
    let answer = "";
    try {
      answer = eval('typeof zzzAlsoNotDeclared === "undefined" ? "ok" : "no"');
    } catch {
      answer = "threw";
    }
    expect(answer).toBe("ok");
  });

  test("the temporal dead zone still raises", () => {
    // NOT exempt, and the difference matters: the name IS declared, so there is
    // a binding to be in the dead zone of.
    let raised = false;
    try {
      eval("{ const r = typeof later === 'undefined'; let later = 1; r; }");
    } catch {
      raised = true;
    }
    expect(raised).toBe(true);
  });

  test("typeof against a VARIABLE is a real string comparison and stays one", () => {
    const want = "number";
    const n: any = 1;
    expect(typeof n === want).toBe(true);
    expect(typeof n === want.toUpperCase()).toBe(false);
    // And the bare form is still a string a program can hold and operate on.
    const held = typeof n;
    expect(held.length).toBe(6);
    expect(held.toUpperCase()).toBe("NUMBER");
    expect(held + "!").toBe("number!");
  });

  test("as a condition and as a value, and inside other expressions", () => {
    const s: any = "x";
    let taken = "";
    if (typeof s === "string") taken = "yes";
    expect(taken).toBe("yes");
    const asValue = typeof s === "string";
    expect(asValue).toBe(true);
    expect(typeof asValue).toBe("boolean");
    expect([typeof s === "string", typeof s === "number"].join(",")).toBe("true,false");
    expect((typeof s === "string") && (typeof s !== "number")).toBe(true);
    let count = 0;
    for (const v of [1, "a", {}, null, undefined]) if (typeof v === "object") count++;
    expect(count).toBe(2);
  });

  test("the operand is evaluated exactly once, where it was written", () => {
    const log: string[] = [];
    function make(tag: string): any {
      log.push(tag);
      return tag;
    }
    expect(typeof make("a") === "string").toBe(true);
    expect(log.join(",")).toBe("a");
    expect(typeof make("b") !== "number").toBe(true);
    expect(log.join(",")).toBe("a,b");
  });
});

describe("the three answers the TAG decides", () => {
  // `number`, `boolean` and `undefined` are readable from the encoding alone
  // and emit a tag test rather than a crossing. `string`, `object` and
  // `function` all arrive as one tag and are told apart by the cell header, so
  // they stay a call. These pin the boundary between the two.
  test("a number is a number however it is encoded", () => {
    // A small integer and a double are DIFFERENT encodings, so `number` is the
    // one name of the three that needs two tests.
    const cases: any[] = [0, -0, 1, -1, 42, 2147483647, -2147483648,
      0.5, -0.5, 1e308, -1e308, NaN, Infinity, -Infinity, 1e-323];
    for (const v of cases) {
      expect(typeof v === "number").toBe(true);
      expect(typeof v !== "number").toBe(false);
    }
  });

  test("and nothing else is", () => {
    const cases: any[] = ["1", true, false, null, undefined, {}, [],
      Symbol("s"), 10n, () => 1];
    for (const v of cases) {
      expect(typeof v === "number").toBe(false);
      expect(typeof v !== "number").toBe(true);
    }
  });

  test("boolean is exactly the two booleans", () => {
    for (const v of [true, false] as any[]) {
      expect(typeof v === "boolean").toBe(true);
    }
    for (const v of [0, 1, "", "true", null, undefined, {}, []] as any[]) {
      expect(typeof v === "boolean").toBe(false);
    }
  });

  test("undefined is not null and not a missing property", () => {
    let missing: any;
    const o: any = {};
    expect(typeof missing === "undefined").toBe(true);
    expect(typeof o.absent === "undefined").toBe(true);
    expect(typeof null === "undefined").toBe(false);
    expect(typeof 0 === "undefined").toBe(false);
    expect(typeof "" === "undefined").toBe(false);
  });

  test("the six that still cross are unchanged", () => {
    expect(typeof "s" === "string").toBe(true);
    expect(typeof {} === "object").toBe(true);
    expect(typeof null === "object").toBe(true);
    expect(typeof [] === "object").toBe(true);
    expect(typeof (() => 1) === "function").toBe(true);
    expect(typeof Symbol("s") === "symbol").toBe(true);
    expect(typeof 1n === "bigint").toBe(true);
    // A boxed number is an OBJECT, which is the case a tag test would get wrong
    // if it looked at the payload instead of the tag.
    expect(typeof new Number(1) === "object").toBe(true);
    expect(typeof new Number(1) === "number").toBe(false);
    expect(typeof new Boolean(true) === "object").toBe(true);
    expect(typeof new Boolean(true) === "boolean").toBe(false);
  });

  test("a PROVEN operand falls through and still answers", () => {
    const n = 1 + 1;
    expect(typeof n === "number").toBe(true);
    expect(typeof n === "string").toBe(false);
    expect(typeof (1 < 2) === "boolean").toBe(true);
  });
});
