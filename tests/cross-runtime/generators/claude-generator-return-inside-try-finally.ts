// Cross-runtime: `return(v)` on a generator suspended inside a `try` runs the
// `finally` block first. Focus: a finally that yields SUSPENDS the return, a
// finally that returns REPLACES the value, and a finally that throws replaces
// the completion entirely.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }
function step(r: any): string { return String(r.value) + ":" + r.done; }

const trace: string[] = [];

// 1) a plain finally runs and the requested value is preserved
trace.length = 0;
function* g1() {
  try {
    yield "a";
    yield "b";
  } finally {
    trace.push("finally1");
  }
  trace.push("after try -- not reached");
}
const i1 = g1();
log("1a=" + step(i1.next()));
log("1b=" + step(i1.return("asked")));
log("1c=" + step(i1.next()));
log("trace1=" + trace.join("|"));

// 2) a finally that YIELDS suspends the return; the generator is alive again
trace.length = 0;
function* g2() {
  try {
    yield "a";
  } finally {
    trace.push("finally2 start");
    yield "cleanup";
    trace.push("finally2 end");
  }
}
const i2 = g2();
log("2a=" + step(i2.next()));
log("2b=" + step(i2.return("asked")));
log("2c=" + step(i2.next()));
log("2d=" + step(i2.next()));
log("trace2=" + trace.join("|"));

// 3) a finally that RETURNS overrides the requested value
function* g3() {
  try {
    yield "a";
  } finally {
    return "fromFinally";
  }
}
const i3 = g3();
log("3a=" + step(i3.next()));
log("3b=" + step(i3.return("asked")));
log("3c=" + step(i3.next()));

// 4) a finally that THROWS turns the return into a throw
function* g4() {
  try {
    yield "a";
  } finally {
    throw new RangeError("fromFinally");
  }
}
const i4 = g4();
log("4a=" + step(i4.next()));
log("4b=" + (function () {
  try { return step(i4.return("asked")); } catch (e: any) { return "threw " + e.constructor.name; }
})());
log("4c=" + step(i4.next()));

// 5) return() on a generator that has NOT started skips the body entirely
trace.length = 0;
function* g5() {
  trace.push("body ran");
  try { yield "a"; } finally { trace.push("finally5"); }
}
const i5 = g5();
log("5a=" + step(i5.return("early")));
log("5b=" + step(i5.next()));
log("trace5=" + JSON.stringify(trace.join("|")));

// 6) return() on an already-completed generator just echoes the value
function* g6() { yield "a"; }
const i6 = g6();
i6.next(); i6.next();
log("6a=" + step(i6.return("echo")));
log("6b=" + step(i6.return(undefined)));

// 7) nested try/finally unwinds from the inside out
trace.length = 0;
function* g7() {
  try {
    try {
      yield "deep";
    } finally {
      trace.push("innerFinally");
    }
  } finally {
    trace.push("outerFinally");
  }
}
const i7 = g7();
i7.next();
log("7a=" + step(i7.return("done")));
log("trace7=" + trace.join("|"));

// 8) a `catch` is NOT entered by return(); only `finally` runs
trace.length = 0;
function* g8() {
  try {
    yield "a";
  } catch (e) {
    trace.push("catch8 -- not reached");
  } finally {
    trace.push("finally8");
  }
}
const i8 = g8();
i8.next();
log("8a=" + step(i8.return("v")));
log("trace8=" + trace.join("|"));

// 9) throw() into a generator suspended in a try WITH a catch resumes there
trace.length = 0;
function* g9() {
  try {
    yield "a";
  } catch (e: any) {
    trace.push("caught " + e.constructor.name);
    yield "recovered";
  } finally {
    trace.push("finally9");
  }
  return "tail";
}
const i9 = g9();
log("9a=" + step(i9.next()));
log("9b=" + step(i9.throw(new TypeError("t"))));
log("9c=" + step(i9.next()));
log("trace9=" + trace.join("|"));

console.log("end");
