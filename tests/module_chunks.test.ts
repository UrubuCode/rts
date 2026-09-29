import { describe, test, expect } from "rts:test";

// A module body large enough that the compiler splits its runs of top-level
// statements into functions (`emit/chunk.rs`). What this pins is that the
// program cannot tell: every top-level statement runs once and in order, a
// binding declared between two runs is the same binding on both sides, a
// hoisted function reads a `const` declared after it, `try`, loops and labels
// behave inside a moved statement, and what refuses the move — `var`, `this`,
// `arguments` — still answers what the language says.

const log: string[] = [];
let counter = 0;
function note(tag: string): number { log.push(tag); return ++counter; }
function readsLater(): number { return LATER * 2; }
const topThis: any = this;

note("a0");
note("a1");
note("a2");
note("a3");
note("a4");
note("a5");
note("a6");
note("a7");
note("a8");
note("a9");
note("a10");
note("a11");
note("a12");
note("a13");
note("a14");
note("a15");
note("a16");
note("a17");
note("a18");
note("a19");
note("a20");
note("a21");
note("a22");
note("a23");
note("a24");
note("a25");
note("a26");
note("a27");
note("a28");
note("a29");
note("a30");
note("a31");
note("a32");
note("a33");
note("a34");
note("a35");
note("a36");
note("a37");
note("a38");
note("a39");
const MIDDLE = counter;
let mutable = 0;
mutable += note("b0") - MIDDLE;
mutable += note("b1") - MIDDLE;
mutable += note("b2") - MIDDLE;
mutable += note("b3") - MIDDLE;
mutable += note("b4") - MIDDLE;
mutable += note("b5") - MIDDLE;
mutable += note("b6") - MIDDLE;
mutable += note("b7") - MIDDLE;
mutable += note("b8") - MIDDLE;
mutable += note("b9") - MIDDLE;
mutable += note("b10") - MIDDLE;
mutable += note("b11") - MIDDLE;
mutable += note("b12") - MIDDLE;
mutable += note("b13") - MIDDLE;
mutable += note("b14") - MIDDLE;
mutable += note("b15") - MIDDLE;
mutable += note("b16") - MIDDLE;
mutable += note("b17") - MIDDLE;
mutable += note("b18") - MIDDLE;
mutable += note("b19") - MIDDLE;
mutable += note("b20") - MIDDLE;
mutable += note("b21") - MIDDLE;
mutable += note("b22") - MIDDLE;
mutable += note("b23") - MIDDLE;
mutable += note("b24") - MIDDLE;
mutable += note("b25") - MIDDLE;
mutable += note("b26") - MIDDLE;
mutable += note("b27") - MIDDLE;
mutable += note("b28") - MIDDLE;
mutable += note("b29") - MIDDLE;
class Box { constructor(public v: number) {} twice(): number { return this.v * 2; } }
const box = new Box(21);
note("c" + box.twice() + ":0");
note("c" + box.twice() + ":1");
note("c" + box.twice() + ":2");
note("c" + box.twice() + ":3");
note("c" + box.twice() + ":4");
note("c" + box.twice() + ":5");
note("c" + box.twice() + ":6");
note("c" + box.twice() + ":7");
note("c" + box.twice() + ":8");
note("c" + box.twice() + ":9");
note("c" + box.twice() + ":10");
note("c" + box.twice() + ":11");
note("c" + box.twice() + ":12");
note("c" + box.twice() + ":13");
note("c" + box.twice() + ":14");
note("c" + box.twice() + ":15");
note("c" + box.twice() + ":16");
note("c" + box.twice() + ":17");
note("c" + box.twice() + ":18");
note("c" + box.twice() + ":19");
let caught = "";
try { note("t0"); throw new Error("inside"); } catch (e: any) { caught = e.message; } finally { note("t1"); }
let loopSum = 0;
outer: for (let i = 0; i < 5; i++) { for (let j = 0; j < 5; j++) { if (j === 3) continue outer; if (i === 4) break outer; loopSum += i * 10 + j; } }
if (loopSum > 0) { note("if"); } else { note("else"); }
switch (loopSum % 3) { case 0: note("s0"); break; case 1: note("s1"); break; default: note("s2"); }
const closures: (() => number)[] = [];
closures.push(() => mutable + 0);
closures.push(() => mutable + 1);
closures.push(() => mutable + 2);
closures.push(() => mutable + 3);
closures.push(() => mutable + 4);
closures.push(() => mutable + 5);
closures.push(() => mutable + 6);
closures.push(() => mutable + 7);
closures.push(() => mutable + 8);
closures.push(() => mutable + 9);
closures.push(() => mutable + 10);
closures.push(() => mutable + 11);
closures.push(() => mutable + 12);
closures.push(() => mutable + 13);
closures.push(() => mutable + 14);
closures.push(() => mutable + 15);
closures.push(() => mutable + 16);
closures.push(() => mutable + 17);
closures.push(() => mutable + 18);
closures.push(() => mutable + 19);
for (var hoisted = 0; hoisted < 3; hoisted++) { note("v" + hoisted); }
var declaredLate = note("var");
note("d0");
note("d1");
note("d2");
note("d3");
note("d4");
note("d5");
note("d6");
note("d7");
note("d8");
note("d9");
note("d10");
note("d11");
note("d12");
note("d13");
note("d14");
note("d15");
note("d16");
note("d17");
note("d18");
note("d19");
const LATER = 21;
const early = readsLater();
const seenThis = typeof this;
const thisInArrow = (() => typeof this)();
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
mutable = mutable + 1;
let stack = "";
function thrower(): never { throw new Error("from a moved statement"); }
try { thrower(); } catch (e: any) { stack = String(e.stack); }
const total = counter;

describe("a module body compiled in pieces", () => {
  test("every top-level statement ran once and in order", () => {
    const expected: string[] = [];
    for (let i = 0; i < 40; i++) expected.push("a" + i);
    for (let i = 0; i < 30; i++) expected.push("b" + i);
    for (let i = 0; i < 20; i++) expected.push("c42:" + i);
    expected.push("t0", "t1", "if");
    expected.push("s" + (loopSum % 3));
    expected.push("v0", "v1", "v2", "var");
    for (let i = 0; i < 20; i++) expected.push("d" + i);
    expect(log.join()).toBe(expected.join());
    expect(total).toBe(expected.length);
  });
  test("bindings declared between the pieces are one binding", () => {
    expect(MIDDLE).toBe(40);
    let sum = 0; for (let i = 1; i <= 30; i++) sum += i;
    expect(mutable).toBe(sum + 20);
    expect(closures.length).toBe(20);
    expect(closures[7]()).toBe(mutable + 7);
    expect(box.twice()).toBe(42);
    expect(early).toBe(42);
    expect(hoisted).toBe(3);
    expect(typeof declaredLate).toBe("number");
  });
  test("control flow inside a moved statement is the statement's own", () => {
    expect(caught).toBe("inside");
    let want = 0;
    for (let i = 0; i < 4; i++) for (let j = 0; j < 3; j++) want += i * 10 + j;
    expect(loopSum).toBe(want);
  });
  test("this at the top level is what it was, and an error still has a trace", () => {
    expect(topThis).toBe(undefined);
    expect(seenThis).toBe("undefined");
    expect(thisInArrow).toBe("undefined");
    expect(stack.includes("from a moved statement")).toBe(true);
    expect(stack.includes("thrower")).toBe(true);
  });
});
