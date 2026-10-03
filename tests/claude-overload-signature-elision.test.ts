// An overload signature is a type, and TypeScript erases it.
//
// A function declaration with no BODY — an overload signature, or a
// `declare function` — names nothing that exists at run time. What stays is the
// implementation, which is written LAST, after every signature that describes
// it. Every expectation below was measured in bun 1.4.0 on 2026-10-03 and the
// engine is asserted against those answers.
//
// Before the fix `parse::item::block_body` turned a missing body into
// `FunctionBody::Block(vec![])`, so each signature became a real function
// returning `undefined`. Three distinct failures came out of that one invented
// empty body, and the middle one is the shape this repository treats as the
// worst there is — a wrong answer with nothing failing:
//
//   - `export function pick(v: number): number;` beside its implementation
//     reached `check::module::duplicate_export` as a second runtime
//     declaration and the file was refused: ``Syntax("`pick` is exported
//     twice")``.
//   - a method's signatures put one key into the class's shape several times,
//     and which function ran depended on how the call was COMPILED.
//     `new K().m("b")` resolved by name and answered the implementation;
//     `const k = new K(); k.m("b")` is a cached access, landed on the first
//     slot, and answered `undefined`. Silently.
//   - `declare function parseInt(…)` bound `parseInt` locally and SHADOWED the
//     global, exactly as `declare const` used to. `parseInt("42")` answered
//     `undefined`. The comment in `parse::stmt::decl` claimed this case was
//     "never affected, because a function with no body is already nothing to
//     emit", and it was the only thing in the repository that said so.
//
// Which function survives is the point of most of this file: in JavaScript the
// types do not exist, so erasing the WRONG declaration leaves a callable of the
// right name and the wrong body, and nothing can see it. Every assertion here
// reads a marker the implementation's body writes.
import { describe, test, expect } from "rts:test";

import * as exported from "./_claude_overload_exported";

// Multiple call signatures on an interface were always type-only, and nothing
// about them changes. Using it as an annotation is what proves it still parses.
interface Overloaded {
  (v: string): string;
  (v: number): number;
}

function loose(v: string): string;
function loose(v: number): number;
function loose(v: any): any {
  return "loose:" + v;
}

// A signature-only declaration with NO implementation anywhere. TypeScript
// reports `Function implementation is missing`; bun erases it, so the name is
// never declared and `typeof` answers `"undefined"` — the exemption the
// specification gives `typeof` for taking a reference rather than a value.
// Calling it is refused at compile time here, by the rule `emit/sloppy.rs`
// applies to every name nothing declares, where bun throws a `TypeError`.
function neverImplemented(v: string): string;

declare function parseInt(s: string): number;

class K {
  tag: string;

  // Two signatures over one body, and the body is last.
  constructor(a: string);
  constructor(a: number);
  constructor(a: any) {
    this.tag = "ctor:" + a;
  }

  m(v: string): string;
  m(v: number): number;
  m(v: any): any {
    return "m:" + v;
  }

  // A second overloaded method in the same body: the erasure is per member, so
  // one class with two of them must keep both implementations.
  n(v: string): string;
  n(v: any): any {
    return "n:" + v;
  }

  static s(v: string): string;
  static s(v: any): any {
    return "s:" + v;
  }

  #p(v: string): string;
  #p(v: any): any {
    return "p:" + v;
  }

  private q(v: string): string;
  private q(v: any): any {
    return "q:" + v;
  }

  callPrivates(v: any): string {
    return this.#p(v) + "/" + this.q(v);
  }
}

describe("an overload signature is erased and the implementation stays", () => {
  test("a local overloaded function runs its body", () => {
    const f: Overloaded = loose;
    expect(f("a")).toBe("loose:a");
    expect(loose(1)).toBe("loose:1");
  });

  test("an EXPORTED overloaded function no longer refuses the module", () => {
    // This is symptom one: the file would not compile at all.
    expect(exported.pick("a")).toBe("a!");
    expect(exported.pick(3)).toBe(6);
  });

  test("`export declare function` puts no key in the namespace", () => {
    expect("elsewhere" in exported).toBe(false);
    expect(Object.keys(exported).join(",")).toBe("pick");
  });

  test("an overloaded method answers through a CACHED access", () => {
    // The `const k` is load-bearing and not style: a property read off a
    // binding the compiler can track is the cached path, and it is the one that
    // answered `undefined` while `new K("z").m("b")` answered correctly.
    const k = new K("z");
    expect(k.m("b")).toBe("m:b");
    expect(k.m(2)).toBe("m:2");
    expect(k.n("f")).toBe("n:f");
  });

  test("an overloaded method answers the same way uncached", () => {
    expect(new K("z").m("b")).toBe("m:b");
  });

  test("an overloaded constructor runs its body", () => {
    expect(new K("z").tag).toBe("ctor:z");
    expect(new K(7).tag).toBe("ctor:7");
  });

  test("overloaded static, `#private` and `private` methods keep their bodies", () => {
    expect(K.s("c")).toBe("s:c");
    const k = new K("z");
    expect(k.callPrivates("d")).toBe("p:d/q:d");
  });

  test("`declare function` does not shadow the global it announces", () => {
    expect(parseInt("42")).toBe(42);
  });

  test("a signature with no implementation declares no name", () => {
    expect(typeof neverImplemented).toBe("undefined");
  });
});
