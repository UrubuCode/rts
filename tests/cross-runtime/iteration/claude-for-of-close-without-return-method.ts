// Cross-runtime: IteratorClose only calls `return` when the iterator HAS one.
// Focus: an iterator with no `return` is abandoned silently, a `return` that
// answers a non-object raises a TypeError on break but is SWALLOWED when the
// loop is leaving because of a throw.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const trace: string[] = [];

function make(tag: string, ret: any) {
  const it: any = {
    i: 0,
    next: function () { it.i++; trace.push(tag + ":next" + it.i); return { value: it.i, done: false }; }
  };
  if (ret !== undefined) it.return = ret;
  return { [Symbol.iterator]: function () { return it; } };
}

// 1) no `return` at all: break just walks away
trace.length = 0;
for (const v of make("a", undefined)) { if (v === 2) break; }
log("1 trace=" + trace.join(","));

// 2) `return` present: it is called with no arguments
trace.length = 0;
for (const v of make("b", function (x: any) { trace.push("b:return:" + String(x)); return { done: true }; })) {
  if (v === 2) break;
}
log("2 trace=" + trace.join(","));

// 3) `return` is looked up but not callable -> TypeError on the break path
trace.length = 0;
log("3 caught=" + (function () {
  try {
    for (const v of make("c", 42)) { if (v === 1) break; }
    return "no";
  } catch (e: any) { return e.constructor.name; }
})());
log("3 trace=" + trace.join(","));

// 4) `return` answers a NON-object -> TypeError on the break path
trace.length = 0;
log("4 caught=" + (function () {
  try {
    for (const v of make("d", function () { trace.push("d:return"); return "notAnObject"; })) { if (v === 1) break; }
    return "no";
  } catch (e: any) { return e.constructor.name; }
})());
log("4 trace=" + trace.join(","));

// 5) leaving because of a THROW: the original error wins, the bad `return` is
//    swallowed
trace.length = 0;
log("5 caught=" + (function () {
  try {
    for (const v of make("e", function () { trace.push("e:return"); return "notAnObject"; })) {
      if (v === 1) throw new RangeError("body");
    }
    return "no";
  } catch (e: any) { return e.constructor.name; }
})());
log("5 trace=" + trace.join(","));

// 6) a `return` that THROWS: on the break path its error surfaces
trace.length = 0;
log("6 caught=" + (function () {
  try {
    for (const v of make("f", function () { trace.push("f:return"); throw new EvalError("fromReturn"); })) {
      if (v === 1) break;
    }
    return "no";
  } catch (e: any) { return e.constructor.name; }
})());

// 7) a `return` that throws while the loop is already throwing: the body's
//    error is the one that escapes
trace.length = 0;
log("7 caught=" + (function () {
  try {
    for (const v of make("g", function () { trace.push("g:return"); throw new EvalError("fromReturn"); })) {
      if (v === 1) throw new URIError("body");
    }
    return "no";
  } catch (e: any) { return e.constructor.name; }
})());
log("7 trace=" + trace.join(","));

// 8) a `return` GETTER is read once, on close only
trace.length = 0;
let reads = 0;
const withGetter: any = {
  [Symbol.iterator]: function () {
    let k = 0;
    return {
      next: function () { k++; trace.push("h:next" + k); return { value: k, done: false }; },
      get return() { reads++; return function () { trace.push("h:return"); return { done: true }; }; }
    };
  }
};
for (const v of withGetter) { if (v === 2) break; }
log("8 reads=" + reads + " trace=" + trace.join(","));

// 9) a normal completion never calls `return`
trace.length = 0;
const finite: any = {
  [Symbol.iterator]: function () {
    let k = 0;
    return {
      next: function () { k++; trace.push("i:next" + k); return k <= 2 ? { value: k, done: false } : { value: undefined, done: true }; },
      return: function () { trace.push("i:return"); return { done: true }; }
    };
  }
};
const seen: number[] = [];
for (const v of finite) seen.push(v);
log("9 seen=" + seen.join(",") + " trace=" + trace.join(","));

// 10) `continue` is not an early exit -- nothing is closed
trace.length = 0;
const kept: number[] = [];
for (const v of finite) { if (v === 1) continue; kept.push(v); }
log("10 kept=" + kept.join(",") + " trace=" + trace.join(","));

// 11) a `return` that answers a well-formed object is accepted and its value
//     is discarded
trace.length = 0;
for (const v of make("j", function () { trace.push("j:return"); return { value: "discarded", done: true }; })) {
  if (v === 1) break;
}
log("11 trace=" + trace.join(","));

// 12) `return` being null is treated as absent
trace.length = 0;
for (const v of make("k", null)) { if (v === 1) break; }
log("12 trace=" + trace.join(","));

console.log("end");
