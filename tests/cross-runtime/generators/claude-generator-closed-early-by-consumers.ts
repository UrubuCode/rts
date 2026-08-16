// Cross-runtime: every consumer that abandons a generator half-way must CLOSE
// it -- for-of on break/return/throw, array destructuring, and Array.from with
// a throwing mapper. Focus: the finally block runs exactly once, and the
// generator is dead afterwards.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const trace: string[] = [];

function* counted(tag: string) {
  try {
    let i = 0;
    while (true) {
      trace.push(tag + ":yield" + i);
      yield i++;
    }
  } finally {
    trace.push(tag + ":closed");
  }
}

// 1) for-of + break
trace.length = 0;
const g1 = counted("break");
for (const v of g1) { if (v === 2) break; }
log("1 trace=" + trace.join("|"));
log("1 dead=" + JSON.stringify(g1.next()));

// 2) for-of + return from an enclosing function
trace.length = 0;
const g2 = counted("return");
function loopReturn() {
  for (const v of g2) { if (v === 1) return "early"; }
  return "full";
}
log("2 result=" + loopReturn());
log("2 trace=" + trace.join("|"));

// 3) for-of whose BODY throws
trace.length = 0;
const g3 = counted("throw");
let caught3 = "no";
try {
  for (const v of g3) { if (v === 1) throw new RangeError("body"); }
} catch (e: any) { caught3 = e.constructor.name; }
log("3 caught=" + caught3 + " trace=" + trace.join("|"));

// 4) array destructuring pulls exactly as many as it binds, then closes
trace.length = 0;
const g4 = counted("destructure");
const [a, b] = g4;
log("4 values=" + a + "," + b);
log("4 trace=" + trace.join("|"));
log("4 dead=" + JSON.stringify(g4.next()));

// 5) a rest element drains to the end, so there is nothing left to close
trace.length = 0;
function* three() {
  try { yield 1; yield 2; yield 3; } finally { trace.push("three:closed"); }
}
const [head, ...rest] = three();
log("5 head=" + head + " rest=" + rest.join(",") + " trace=" + trace.join("|"));

// 6) destructuring with a HOLE still counts the pull
trace.length = 0;
const g6 = counted("hole");
const [, , third] = g6;
log("6 third=" + third + " trace=" + trace.join("|"));

// 7) a default value that throws closes the iterator
trace.length = 0;
function* two() {
  try { yield "x"; } finally { trace.push("two:closed"); }
}
let caught7 = "no";
try {
  const [p, q = (function () { throw new TypeError("default"); })()] = two();
  log("7 unreachable " + p + q);
} catch (e: any) { caught7 = e.constructor.name; }
log("7 caught=" + caught7 + " trace=" + trace.join("|"));

// 8) Array.from with a throwing mapper closes the source
trace.length = 0;
const g8 = counted("arrayFrom");
let caught8 = "no";
try {
  Array.from(g8, function (v: any) { if (v === 1) throw new EvalError("map"); return v; });
} catch (e: any) { caught8 = e.constructor.name; }
log("8 caught=" + caught8 + " trace=" + trace.join("|"));

// 9) a generator that RETURNS normally still runs finally, once
trace.length = 0;
const g9 = three();
const drained: number[] = [];
for (const v of g9) drained.push(v);
log("9 drained=" + drained.join(",") + " trace=" + trace.join("|"));
log("9 extraReturn=" + JSON.stringify(g9.return(0 as any)));
log("9 traceAfter=" + trace.join("|"));

// 10) a generator closed by break inside a NESTED loop closes only its own
trace.length = 0;
const outerG = counted("outer");
const innerG = counted("inner");
outer:
for (const o of outerG) {
  for (const i2 of innerG) {
    if (i2 === 1) break outer;
  }
}
log("10 trace=" + trace.join("|"));

// 11) spread drains fully, so `closed` comes from the normal completion
trace.length = 0;
const spread = [...three()];
log("11 spread=" + spread.join(",") + " trace=" + trace.join("|"));

console.log("end");
