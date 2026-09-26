import { describe, test, expect } from "rts:test";

// A function declared inside another and only ever called is substituted at each
// call, and so is a local function passed to it by name and called there. What a
// substitution must not change is what each call means -- these are the cases
// where it could.

function passedAndCalled(n: number): number {
  function apply(g: (x: number) => number, x: number): number { return g(x); }
  const inc = (x: number) => x + 1;
  let a = 0;
  for (let i = 0; i < n; i++) a = apply(inc, a) | 0;
  return a;
}

// Called above its declaration, which hoisting puts in force first.
function calledAboveItsDeclaration(): number {
  const r = twice(4);
  function twice(x: number): number { return x * 2; }
  return r;
}

// A recursive one stays a call where it names itself.
function recursive(): number {
  function down(n: number): number { return n > 0 ? down(n - 1) + 1 : 0; }
  return down(5);
}

// Its `arguments` is its own and not the caller's.
function readsItsOwnArguments(p: number, q: number): number {
  function count(a: number): number { return arguments.length; }
  return count(p) * 10 + count(p, q);
}

// A name written after the declaration is not the declared function.
function writtenAfter(): number {
  function h(): number { return 1; }
  let s = h();
  (h as any) = () => 2;
  s = s * 10 + h();
  return s;
}

// A free name read at the call, not where it was declared.
function readsAtTheCall(): string {
  let k = 1;
  function plusK(x: number): number { return x + k; }
  const first = plusK(1);
  k = 10;
  return first + "," + plusK(1);
}

// Arguments are evaluated once and in order, and a missing one is undefined.
function argumentsOnceInOrder(): string {
  const seen: number[] = [];
  const note = (v: number) => (seen.push(v), v);
  function pair(a: number, b: number): string { return a + ":" + b; }
  const r = pair(note(1), note(2));
  function second(a: number, b?: number): string { return "" + b; }
  return r + "|" + seen.join() + "|" + second(note(3));
}

// A parameter passed on through a second substitution still names the function.
function passedTwice(): number {
  function call1(f: (x: number) => number, x: number): number { return f(x); }
  function call2(f: (x: number) => number, x: number): number { return call1(f, x) + call1(f, 0); }
  const sq = (x: number) => x * x;
  return call2(sq, 3);
}

// An argument that only looks like the function -- a parameter of the same spelling
// bound to something else -- is called as what it holds.
function shadowedSpelling(): number {
  const inc = (x: number) => x + 1;
  function apply(inc: (x: number) => number, x: number): number { return inc(x); }
  const dbl = (x: number) => x * 2;
  return apply(dbl, 5) * 100 + apply(inc, 5);
}

// Read as a value and still called: the binding is never written, so each call is
// the function declared.
let KEPT: unknown;
function readAsAValue(n: number): number {
  function f(x: number, y: number = 1): number { return x + y; }
  KEPT = f;
  let a = 0;
  for (let i = 0; i < n; i++) a = f(a) | 0;
  return a;
}

// A default applies where the argument is missing or undefined, and nowhere else,
// and reads the parameters before it.
function defaults(): string {
  function g(a: number, b: number = a * 10, c: any = "c"): string { return a + "," + b + "," + c; }
  return [g(1), g(1, 2), g(1, undefined, null), g(1, 0, 0)].join("|");
}

// A default runs only where it applies.
function defaultRunsOnlyWhenNeeded(): string {
  let runs = 0;
  const tick = () => ++runs;
  function h(x: number, y: number = tick()): number { return x + y; }
  const r = [h(1, 5), h(1), h(1, undefined)];
  return r.join() + "|" + runs;
}

// Written from inside a closure, which the call cannot see.
function writtenByAClosure(): number {
  function k(): number { return 1; }
  const swap = () => { (k as any) = () => 2; };
  const first = k();
  swap();
  return first * 10 + k();
}

// A plain and a logical assignment, each a write the scope tree has to see.
function writtenPlainly(): number {
  function k(): number { return 1; }
  const swap = () => { k = () => 2; };
  const first = k();
  swap();
  return first * 10 + k();
}
function writtenLogically(): number {
  function k(): number { return 1; }
  const swap = () => { (k as any) ||= 0; (k as any) &&= () => 6; };
  const first = k();
  swap();
  return first * 10 + k();
}

describe("local function substitution", () => {
  test("written plainly, and by a logical assignment", () => {
    expect(writtenPlainly()).toBe(12);
    expect(writtenLogically()).toBe(16);
  });
  test("read as a value, and called", () => {
    expect(readAsAValue(1000)).toBe(1000);
    expect((KEPT as any)(1)).toBe(2);
  });
  test("defaults where missing or undefined", () => {
    expect(defaults()).toBe("1,10,c|1,2,c|1,10,null|1,0,0");
  });
  test("a default runs only where it applies", () => {
    expect(defaultRunsOnlyWhenNeeded()).toBe("6,2,3|2");
  });
  test("written by a closure", () => {
    expect(writtenByAClosure()).toBe(12);
  });
  test("a function passed to another and called there", () => {
    expect(passedAndCalled(1000)).toBe(1000);
  });
  test("a call above the declaration", () => {
    expect(calledAboveItsDeclaration()).toBe(8);
  });
  test("recursion", () => {
    expect(recursive()).toBe(5);
  });
  test("arguments is the declared function's", () => {
    expect(readsItsOwnArguments(1, 2)).toBe(12);
  });
  test("a binding written later", () => {
    expect(writtenAfter()).toBe(12);
  });
  test("a free name is read at the call", () => {
    expect(readsAtTheCall()).toBe("2,11");
  });
  test("arguments once, in order, missing ones undefined", () => {
    expect(argumentsOnceInOrder()).toBe("1:2|1,2|undefined");
  });
  test("a function passed through two substitutions", () => {
    expect(passedTwice()).toBe(9);
  });
  test("a parameter spelled like a local function", () => {
    expect(shadowedSpelling()).toBe(1006);
  });
});
