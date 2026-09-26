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

describe("local function substitution", () => {
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
