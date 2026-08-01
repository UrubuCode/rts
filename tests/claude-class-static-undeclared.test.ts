import { describe, test, expect } from "rts:test";

// A class property that the class BODY never declares — what a minifier emits
// when it hangs shared state off a constructor (`class e {}; e.$8 = new Map()`).
// It has no compile-time cell, so it lives in the per-class runtime statics
// object; reads walk the compile-time parent chain, writes always land on the
// named class (JS assignment creates an OWN property that shadows the parent).
// Every expectation below was checked against Node.

class Bare {}
Bare.$8 = 42;
const bareRead = Bare.$8;

class BareChild extends Bare {}
const inheritedUndeclared = BareChild.$8;
BareChild.$8 = 7;
const shadowedUndeclared = BareChild.$8;
const parentUntouched = Bare.$8;

class Absent {}
const missing = Absent.nope;

// Declared statics INHERIT (JS: `D`'s [[Prototype]] is the constructor `C`).
class Declared {
  static a: number = 1;
  static m(): number {
    return 2;
  }
}
class DeclaredChild extends Declared {}
const inheritedField = DeclaredChild.a;
const inheritedMethod = DeclaredChild.m();

// …and a write through the subclass SHADOWS, it does not mutate the parent.
class Shadowed extends Declared {}
Shadowed.a = 9;
const shadowedField = Shadowed.a;
const declaredParentUntouched = Declared.a;

// Two levels of inheritance, and a non-string value.
class L1 {}
L1.tag = "root";
class L2 extends L1 {}
class L3 extends L2 {}
const twoLevels = L3.tag;

class Holder {}
Holder.list = [1, 2, 3];
const holderLen = Holder.list.length;

// The same property through a class VALUE (an alias, or a class passed as an
// argument) must agree with the bare-name spelling: a class's statics live with
// the class, not in the reified function value.
class Aliased {}
Aliased.n = 5;
const K = Aliased;
const viaAlias = K.n;
function readN(c: any): any {
  return c.n;
}
const viaParam = readN(Aliased);
K.n = 6;
const viaAliasWrite = Aliased.n;

describe("undeclared static class properties", () => {
  test("a property assigned outside the class body reads back", () => {
    expect(bareRead).toBe(42);
  });

  test("a subclass sees the parent's undeclared property", () => {
    expect(inheritedUndeclared).toBe(42);
  });

  test("writing through the subclass shadows instead of mutating the parent", () => {
    expect(shadowedUndeclared).toBe(7);
    expect(parentUntouched).toBe(42);
  });

  test("a property nobody ever set is undefined, not an error", () => {
    expect(missing).toBe(undefined);
  });

  test("declared static fields and methods inherit", () => {
    expect(inheritedField).toBe(1);
    expect(inheritedMethod).toBe(2);
  });

  test("a subclass write shadows an inherited DECLARED static field", () => {
    expect(shadowedField).toBe(9);
    expect(declaredParentUntouched).toBe(1);
  });

  test("the walk crosses more than one level", () => {
    expect(twoLevels).toBe("root");
  });

  test("the value can be any type, not just a number", () => {
    expect(holderLen).toBe(3);
  });

  test("a class VALUE reaches the same statics as the class name", () => {
    expect(viaAlias).toBe(5);
    expect(viaParam).toBe(5);
    expect(viaAliasWrite).toBe(6);
  });
});
