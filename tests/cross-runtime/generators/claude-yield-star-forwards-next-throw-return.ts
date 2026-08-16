// Cross-runtime: `yield*` is a CONDUIT. next(v), throw(e) and return(v) sent to
// the outer generator must reach the INNER one while the delegation is live,
// and the inner generator's `return` value is what the `yield*` expression
// evaluates to.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }
function step(r: any): string { return String(r.value) + ":" + r.done; }

const trace: string[] = [];

// 1) next(v) travels through the delegation into the inner generator
function* inner1() {
  const a = yield "i1";
  trace.push("inner got " + a);
  const b = yield "i2";
  trace.push("inner got " + b);
  return "innerReturn";
}
function* outer1() {
  const before = yield "o1";
  trace.push("outer got " + before);
  const fromStar = yield* inner1();
  trace.push("star produced " + fromStar);
  const after = yield "o2";
  trace.push("outer got " + after);
  return "outerReturn";
}
const g1 = outer1();
log("g1a=" + step(g1.next("ignored-first")));
log("g1b=" + step(g1.next("A")));
log("g1c=" + step(g1.next("B")));
log("g1d=" + step(g1.next("C")));
log("g1e=" + step(g1.next("D")));
log("g1f=" + step(g1.next("E")));
log("trace1=" + trace.join("|"));

// 2) throw() lands inside the INNER generator, which may catch and carry on
trace.length = 0;
function* inner2() {
  try {
    yield "i1";
    trace.push("inner not reached");
  } catch (e: any) {
    trace.push("inner caught " + e.constructor.name);
    yield "i-recovered";
  } finally {
    trace.push("inner finally");
  }
  return "innerDone";
}
function* outer2() {
  try {
    const v = yield* inner2();
    trace.push("star produced " + v);
  } catch (e: any) {
    trace.push("outer caught " + e.constructor.name);
  }
  yield "o-last";
  return "outerDone";
}
const g2 = outer2();
log("g2a=" + step(g2.next()));
log("g2b=" + step(g2.throw(new RangeError("boom"))));
log("g2c=" + step(g2.next()));
log("g2d=" + step(g2.next()));
log("trace2=" + trace.join("|"));

// 3) an inner generator with no catch lets the throw escape to the OUTER one
trace.length = 0;
function* inner3() {
  try {
    yield "i1";
  } finally {
    trace.push("inner3 finally");
  }
}
function* outer3() {
  try {
    yield* inner3();
  } catch (e: any) {
    trace.push("outer3 caught " + e.constructor.name);
    yield "o-recovered";
  }
  return "outer3Done";
}
const g3 = outer3();
log("g3a=" + step(g3.next()));
log("g3b=" + step(g3.throw(new TypeError("t"))));
log("g3c=" + step(g3.next()));
log("trace3=" + trace.join("|"));

// 4) return() closes the inner generator first; its finally runs
trace.length = 0;
function* inner4() {
  try {
    yield "i1";
    yield "i2";
  } finally {
    trace.push("inner4 finally");
  }
}
function* outer4() {
  try {
    yield* inner4();
  } finally {
    trace.push("outer4 finally");
  }
}
const g4 = outer4();
log("g4a=" + step(g4.next()));
log("g4b=" + step(g4.return("closed")));
log("g4c=" + step(g4.next()));
log("trace4=" + trace.join("|"));

// 5) the value of a `yield*` over an array is undefined -- an array iterator
//    returns no value
function* outer5() {
  const v = yield* [10, 20];
  return "arrayStar=" + String(v);
}
const g5 = outer5();
log("g5a=" + step(g5.next()));
log("g5b=" + step(g5.next()));
log("g5c=" + step(g5.next()));

// 6) nested delegation forwards through BOTH levels
trace.length = 0;
function* deep() {
  const x = yield "d1";
  trace.push("deep got " + x);
  return "deepReturn";
}
function* mid() { return "mid:" + (yield* deep()); }
function* top() { return "top:" + (yield* mid()); }
const g6 = top();
log("g6a=" + step(g6.next()));
log("g6b=" + step(g6.next("through")));
log("trace6=" + trace.join("|"));

console.log("end");
