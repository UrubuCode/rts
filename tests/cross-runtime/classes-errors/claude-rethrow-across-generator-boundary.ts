// Cross-runtime: gen.throw() resumes the generator AT the yield with an
// exception, gen.return() runs pending finally blocks, and a value returned
// from a finally overrides the requested return — the same completion rules the
// plain try/finally has, observed through the iterator protocol.
function* guarded(): Generator<string, string, any> {
  try {
    yield "a";
    yield "b";
    return "normal-end";
  } catch (e: any) {
    yield "caught:" + e.constructor.name;
    return "after-catch";
  } finally {
    trace.push("finally");
  }
}
const trace: string[] = [];

const g1 = guarded();
console.log("g1-1=" + JSON.stringify(g1.next()));
console.log("g1-throw=" + JSON.stringify(g1.throw(new RangeError("x"))));
console.log("g1-2=" + JSON.stringify(g1.next()));
console.log("g1-3=" + JSON.stringify(g1.next()));
console.log("g1-trace=" + trace.join(","));

// throw() before the first next() is not caught by the body: the generator
// never started, so it propagates and the generator ends.
trace.length = 0;
const g2 = guarded();
let out = "none";
try {
  g2.throw(new TypeError("early"));
} catch (e: any) {
  out = e.constructor.name;
}
console.log("g2-early=" + out);
console.log("g2-trace=" + trace.join(","));
console.log("g2-after=" + JSON.stringify(g2.next()));

// return() runs the finally, and the returned value is the one requested.
trace.length = 0;
const g3 = guarded();
g3.next();
console.log("g3-return=" + JSON.stringify(g3.return("early-exit")));
console.log("g3-trace=" + trace.join(","));
console.log("g3-after=" + JSON.stringify(g3.next()));

// A finally that yields defers the return; a finally that returns overrides it.
function* stubborn(): Generator<string, string, any> {
  try {
    yield "work";
    return "unreached";
  } finally {
    yield "cleanup";
  }
}
const g4 = stubborn();
console.log("g4-1=" + JSON.stringify(g4.next()));
console.log("g4-return=" + JSON.stringify(g4.return("wanted")));
console.log("g4-2=" + JSON.stringify(g4.next()));
console.log("g4-3=" + JSON.stringify(g4.next()));

function* overriding(): Generator<string, string, any> {
  try {
    yield "w";
    return "unreached";
  } finally {
    return "from-finally";
  }
}
const g5 = overriding();
console.log("g5-1=" + JSON.stringify(g5.next()));
console.log("g5-return=" + JSON.stringify(g5.return("wanted")));

function* throwingFinally(): Generator<string, string, any> {
  try {
    yield "w";
    return "unreached";
  } finally {
    throw new EvalError("from-finally");
  }
}
const g6 = throwingFinally();
g6.next();
let g6out = "none";
try {
  g6.return("wanted");
} catch (e: any) {
  g6out = e.constructor.name;
}
console.log("g6-return=" + g6out);
console.log("g6-after=" + JSON.stringify(g6.next()));

// yield* forwards throw() and return() to the inner iterator.
const inner: string[] = [];
function* innerGen(): Generator<string, string, any> {
  try {
    yield "i1";
    yield "i2";
    return "inner-done";
  } catch (e: any) {
    inner.push("inner-caught:" + e.constructor.name);
    return "inner-recovered";
  } finally {
    inner.push("inner-finally");
  }
}
function* outerGen(): Generator<string, string, any> {
  const v = yield* innerGen();
  yield "outer-saw:" + v;
  return "outer-done";
}
const g7 = outerGen();
console.log("g7-1=" + JSON.stringify(g7.next()));
console.log("g7-throw=" + JSON.stringify(g7.throw(new RangeError("d"))));
console.log("g7-2=" + JSON.stringify(g7.next()));
console.log("g7-inner=" + inner.join(","));

inner.length = 0;
const g8 = outerGen();
g8.next();
console.log("g8-return=" + JSON.stringify(g8.return("stop")));
console.log("g8-inner=" + inner.join(","));

// An exception thrown inside a generator body propagates to the CALLER of next.
function* explodes(): Generator<number> {
  yield 1;
  throw new URIError("boom");
}
const g9 = explodes();
console.log("g9-1=" + JSON.stringify(g9.next()));
let g9out = "none";
try {
  g9.next();
} catch (e: any) {
  g9out = e.constructor.name;
}
console.log("g9-throw=" + g9out);
console.log("g9-done=" + JSON.stringify(g9.next()));

// The same shapes through async: an awaited rejection is caught by try/catch.
const asyncLog: string[] = [];
async function rejects(): Promise<string> {
  throw new RangeError("async");
}
async function wrapper(): Promise<string> {
  try {
    await rejects();
    return "no-throw";
  } catch (e: any) {
    asyncLog.push("caught:" + e.constructor.name);
    throw new TypeError("rewrapped");
  } finally {
    asyncLog.push("finally");
  }
}
wrapper().then(
  () => {
    asyncLog.push("resolved");
  },
  (e: any) => {
    asyncLog.push("rejected:" + e.constructor.name);
  },
).then(() => {
  console.log("async=" + asyncLog.join("|"));
});
console.log("sync-tail=reached");
