// Cross-runtime: the ES2025 helpers validate their arguments EAGERLY, at the
// call that builds them, and a rejected argument CLOSES the source. take/drop
// coerce their count with ToIntegerOrInfinity; NaN and negatives are RangeError.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

function* src(tag: string) {
  try {
    let i = 0;
    while (i < 10) { pulls.push(tag + i); yield i++; }
  } finally {
    pulls.push(tag + "-closed");
  }
}

function attempt(fn: () => any): string {
  try { const v = fn(); return "ok:" + (v === undefined ? "undefined" : typeof v); }
  catch (e: any) { return e.constructor.name; }
}

// 1) a non-callable mapper is refused when map() is CALLED, not on first next()
pulls.length = 0;
log("mapNonCallable=" + attempt(function () { return (src("a") as any).map(42); }));
log("mapClosedSource=" + JSON.stringify(pulls.join(",")));

// 2) the same for filter, flatMap, and the terminal helpers
pulls.length = 0;
log("filterNonCallable=" + attempt(function () { return (src("b") as any).filter(null); }));
log("flatMapNonCallable=" + attempt(function () { return (src("c") as any).flatMap("nope"); }));
log("someNonCallable=" + attempt(function () { return (src("d") as any).some({}); }));
log("everyNonCallable=" + attempt(function () { return (src("e") as any).every(undefined); }));
log("findNonCallable=" + attempt(function () { return (src("f") as any).find(0); }));
log("forEachNonCallable=" + attempt(function () { return (src("g") as any).forEach(1); }));
log("reduceNonCallable=" + attempt(function () { return (src("h") as any).reduce("x"); }));
log("closedByEach=" + pulls.join(","));

// 3) take/drop counts: NaN and negatives are RangeError, not TypeError
pulls.length = 0;
log("takeNaN=" + attempt(function () { return (src("i") as any).take(NaN); }));
log("takeNegative=" + attempt(function () { return (src("j") as any).take(-1); }));
log("dropNaN=" + attempt(function () { return (src("k") as any).drop(NaN); }));
log("dropNegative=" + attempt(function () { return (src("l") as any).drop(-5); }));
log("rangeErrorsClosed=" + pulls.join(","));

// 4) a count that coerces: strings, booleans, null, undefined, fractions
log("takeString=" + (src("m") as any).take("2").toArray().join(","));
log("takeTrue=" + (src("n") as any).take(true).toArray().join(","));
log("takeNull=" + JSON.stringify((src("o") as any).take(null).toArray().join(",")));
log("takeFraction=" + (src("p") as any).take(2.9).toArray().join(","));
log("takeUndefined=" + attempt(function () { return (src("q") as any).take(undefined); }));

// 5) Infinity is accepted by both: take(Infinity) never truncates, drop(Infinity)
//    drains everything
function* three(tag: string) { let i = 1; while (i <= 3) { pulls.push(tag + i); yield i++; } }
log("takeInfinity=" + (three("r") as any).take(Infinity).toArray().join(","));
log("dropInfinity=" + JSON.stringify((three("s") as any).drop(Infinity).toArray().join(",")));

// 6) drop past the end leaves nothing; drop(0) leaves everything
log("dropPastEnd=" + JSON.stringify((three("t") as any).drop(99).toArray().join(",")));
log("dropZero=" + (three("u") as any).drop(0).toArray().join(","));

// 7) a count whose valueOf throws propagates that error, and closes the source
pulls.length = 0;
const badCount = { valueOf: function () { throw new RangeError("no"); } };
log("countValueOfThrows=" + attempt(function () { return (src("v") as any).take(badCount); }));
log("afterBadCount=" + pulls.join(","));

// 8) a count whose valueOf answers a number is honoured
log("countValueOf=" + (src("w") as any).take({ valueOf: function () { return 2; } }).toArray().join(","));

// 9) a Symbol count is a TypeError from the coercion, not a RangeError
log("symbolCount=" + attempt(function () { return (src("x") as any).take(Symbol.iterator); }));

// 10) an extra argument is ignored; a missing one is undefined and refused
log("extraArgIgnored=" + (src("y") as any).take(2, "ignored").toArray().join(","));
log("mapNoArg=" + attempt(function () { return (src("z") as any).map(); }));

// 11) reduce's non-callable check happens before the seed is looked at
log("reduceNonCallableWithSeed=" + attempt(function () { return (src("A") as any).reduce(null, 0); }));

console.log("end");
