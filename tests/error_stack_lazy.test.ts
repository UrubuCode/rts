// `new Error()` cost 790 ns against 110 for `new Map()` and 60 for a plain class
// instance, and 690 of those were `.stack`: about 320 to render it — two
// `format!` and a walk of the call stack — and about 370 to intern the result
// and write the property. Every construction paid it, and almost nothing reads
// `.stack`.
//
// Measured by ablation, release, min of 9 over 100 K iterations:
//
//   return immediately after `receiver`             100 ns
//   the stack RENDERED, not interned or written     420 ns
//   the whole constructor                           790 ns
//
// What is captured now is the call stack as a `Vec<u64>` of code addresses and
// the class name. `Error.prototype` carries a `stack` accessor that renders on
// the first read, installs an own data property, and drops the capture — so a
// second read is an ordinary cached read.
//
//   new Error("x")           1030 -> 580
//   throw an Error + catch   1020 -> 570
//   new Error + READ .stack  1020 -> 1480      <- STATED, not hidden
//
// The work moved rather than vanished: a program that reads `.stack` pays more
// than it did. That is the trade, and it is taken because a caught exception
// that never looks at its own stack is the ordinary case.
import { describe, test, expect } from "rts:test";

const NL = String.fromCharCode(10);

describe("what a deferred stack must still answer", () => {
  test("the frames are the ones where the error was CONSTRUCTED", () => {
    function inner(): never {
      throw new Error("boom");
    }
    function middle(): void {
      inner();
    }
    let seen = "";
    try {
      middle();
    } catch (err) {
      seen = String((err as Error).stack);
    }
    expect(seen.indexOf("at inner") >= 0).toBe(true);
  });

  test("the header is `Name: message`", () => {
    const e = new Error("msg");
    expect(String(e.stack).split(NL)[0]).toBe("Error: msg");
  });

  test("a subclass names itself", () => {
    const e = new TypeError("t");
    expect(String(e.stack).split(NL)[0]).toBe("TypeError: t");
  });

  test("a renamed error uses the NAME it has when read", () => {
    // The header is built at read time from the properties, which is what makes
    // this answer "Mine" — and what node answers. Building it at construction
    // could not.
    const e = new Error("n");
    e.name = "Mine";
    expect(String(e.stack).split(NL)[0]).toBe("Mine: n");
  });

  test("read twice, the same string", () => {
    // The first read installs an own property and drops the capture; the second
    // must find it rather than render again or answer `undefined`.
    const e = new Error("twice");
    expect(e.stack === e.stack).toBe(true);
    expect(String(e.stack).indexOf("Error: twice")).toBe(0);
  });

  test("written, and the write wins", () => {
    const e = new Error("w");
    (e as any).stack = "replaced";
    expect(e.stack).toBe("replaced");
  });

  test("written BEFORE any read", () => {
    // The setter has to drop the capture, or a later read would render over it.
    const e = new Error("w2");
    (e as any).stack = "early";
    expect(e.stack).toBe("early");
  });

  test("`stack` is reachable through the chain", () => {
    expect("stack" in new Error("i")).toBe(true);
  });

  test("and it is not enumerable", () => {
    expect(JSON.stringify(Object.keys(new Error("k")))).toBe("[]");
  });

  test("an error the ENGINE throws also carries one", () => {
    // A `TypeError` this engine raises reaches the class through a different
    // registration path than a program naming `Error` does, and installing the
    // accessor at registration left that path without one — 332 tests sharing
    // one process read `undefined` from every stack. It is installed at
    // construction now, where the order cannot matter.
    let seen = "";
    try {
      (undefined as any).x();
    } catch (err) {
      seen = String((err as Error).stack);
    }
    expect(seen.indexOf("TypeError") >= 0).toBe(true);
  });
});
