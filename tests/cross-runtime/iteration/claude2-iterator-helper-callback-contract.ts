// Cross-runtime: what a helper callback is CALLED WITH -- (value, index) and
// nothing else, `this` undefined, the index counting source positions -- and
// what a throwing callback does to the source.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

function* src(tag: string, values: any[]) {
  try {
    for (let i = 0; i < values.length; i++) { pulls.push(tag + i); yield values[i]; }
  } finally {
    pulls.push(tag + "-closed");
  }
}

function attempt(fn: () => any): string {
  try { return "ok:" + String(fn()); } catch (e: any) { return e.constructor.name; }
}

// 1) map's callback gets exactly two arguments
const seen1: string[] = [];
src("a", ["x", "y", "z"]).map(function (v: any, i: number) {
  seen1.push(v + "@" + i + "#" + arguments.length);
  return v;
}).toArray();
log("mapArgs=" + seen1.join(","));

// 2) filter's index counts every SOURCE value, not the surviving ones
const seen2: string[] = [];
const kept = src("b", [1, 2, 3, 4, 5]).filter(function (v: any, i: number) {
  seen2.push(v + "@" + i);
  return v % 2 === 1;
}).toArray();
log("filterArgs=" + seen2.join(",") + " kept=" + kept.join(","));

// 3) an index does NOT restart after a filter: map after filter numbers what it
//    actually receives
const seen3: string[] = [];
src("c", [1, 2, 3, 4]).filter(function (v: any) { return v % 2 === 0; })
  .map(function (v: any, i: number) { seen3.push(v + "@" + i); return v; }).toArray();
log("chainedIndex=" + seen3.join(","));

// 4) take and drop shift the index the downstream helper sees
const seen4: string[] = [];
src("d", [10, 20, 30, 40]).drop(2).map(function (v: any, i: number) { seen4.push(v + "@" + i); return v; }).toArray();
log("afterDrop=" + seen4.join(","));

// 5) the terminal helpers pass an index too
const seen5: string[] = [];
src("e", ["p", "q"]).forEach(function (v: any, i: number) { seen5.push(v + "@" + i + "#" + arguments.length); });
log("forEachArgs=" + seen5.join(","));
const seen5b: string[] = [];
src("f", ["p", "q", "r"]).some(function (v: any, i: number) { seen5b.push(v + "@" + i); return v === "q"; });
log("someArgs=" + seen5b.join(","));
const seen5c: string[] = [];
src("g", [1, 2, 3]).reduce(function (acc: any, v: any, i: number) { seen5c.push(v + "@" + i + "#" + arguments.length); return acc; }, "S");
log("reduceArgs=" + seen5c.join(","));

// 6) reduce WITHOUT a seed starts the index at 1, because element 0 is the seed
const seen6: string[] = [];
const total = src("h", [1, 2, 3]).reduce(function (acc: any, v: any, i: number) { seen6.push(v + "@" + i); return acc + v; });
log("reduceNoSeed=" + seen6.join(",") + " total=" + total);

// 7) the callback is invoked with NO receiver: `this` is neither the helper nor
//    the source. (Its exact value is not asserted -- an undefined receiver
//    becomes globalThis in a sloppy-mode script and stays undefined in a strict
//    one, which is a property of the CALLER's mode, not of the helper.)
let thisWasHelper = "unset";
let thisWasSource = "unset";
const source7 = src("i", [1]);
const helper7: any = source7.map(function (this: any) {
  thisWasHelper = String(this === helper7);
  thisWasSource = String(this === source7);
  return 1;
});
helper7.toArray();
log("callbackThisIsHelper=" + thisWasHelper + " isSource=" + thisWasSource);

// 8) a callback that throws propagates and CLOSES the source
pulls.length = 0;
log("mapThrows=" + attempt(function () {
  return src("j", [1, 2, 3]).map(function (v: any) { if (v === 2) throw new RangeError("x"); return v; }).toArray();
}));
log("mapThrowPulls=" + pulls.join(","));

// 9) the same for a predicate and for a terminal helper
pulls.length = 0;
log("filterThrows=" + attempt(function () {
  return src("k", [1, 2, 3]).filter(function (v: any) { if (v === 2) throw new RangeError("x"); return true; }).toArray();
}));
log("someThrows=" + attempt(function () {
  return src("l", [1, 2, 3]).some(function (v: any) { if (v === 2) throw new RangeError("x"); return false; });
}));
log("reduceThrows=" + attempt(function () {
  return src("m", [1, 2, 3]).reduce(function (a: any, v: any) { if (v === 2) throw new RangeError("x"); return a; }, 0);
}));
log("throwPulls=" + pulls.join(","));

// 10) a source whose next() answers a NON-OBJECT is a TypeError at the pull
const badNext: any = { next: function () { return 42; } };
log("nonObjectResult=" + attempt(function () { return (Iterator as any).from(badNext).map(function (v: any) { return v; }).next(); }));

// 11) a source whose next() THROWS lets the error through unchanged
const throwingNext: any = { next: function () { throw new EvalError("boom"); } };
log("throwingNext=" + attempt(function () { return (Iterator as any).from(throwingNext).map(function (v: any) { return v; }).next(); }));

// 12) `done` is coerced as a boolean: a truthy non-boolean ends the iteration
let calls = 0;
const truthyDone: any = {
  next: function () { calls++; return calls === 1 ? { done: 0, value: "v1" } : { done: "yes", value: "ignored" }; }
};
log("truthyDone=" + JSON.stringify((Iterator as any).from(truthyDone).toArray()) + " calls=" + calls);

// 13) the value of a done result is dropped by the helpers
const doneValue: any = {
  i: 0,
  next: function () { this.i++; return this.i <= 2 ? { done: false, value: "d" + this.i } : { done: true, value: "FINAL" }; }
};
log("doneValueDropped=" + JSON.stringify((Iterator as any).from(doneValue).map(function (v: any) { return v; }).toArray()));

console.log("end");
