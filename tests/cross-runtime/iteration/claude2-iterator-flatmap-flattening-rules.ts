// Cross-runtime: what `flatMap` accepts back from its callback. An iterable or
// an iterator is flattened one level; a PRIMITIVE STRING is refused even though
// strings are iterable; anything else is a TypeError that closes the source.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

function* letters(tag: string, values: any[]) {
  try {
    for (let i = 0; i < values.length; i++) { pulls.push(tag + i); yield values[i]; }
  } finally {
    pulls.push(tag + "-closed");
  }
}

function attempt(fn: () => any): string {
  try { return "ok:" + String(fn()); } catch (e: any) { return e.constructor.name; }
}

// 1) an array return is flattened one level -- nested arrays stay nested
log("arrays=" + JSON.stringify(letters("a", [1, 2]).flatMap(function (v: any) { return [v, [v, v]]; }).toArray()));

// 2) an empty array contributes nothing
log("empties=" + JSON.stringify(letters("b", [1, 2, 3]).flatMap(function (v: any) { return v === 2 ? [] : [v]; }).toArray()));

// 3) a generator return works, and is drained before the next source value
pulls.length = 0;
function* pair(v: any) { yield v + "-x"; yield v + "-y"; }
log("generators=" + letters("c", ["p", "q"]).flatMap(function (v: any) { return pair(v); }).toArray().join(","));
log("generatorPulls=" + pulls.join(","));

// 4) a bare ITERATOR (a `next`-only object) is accepted -- GetIteratorFlattenable
//    falls back to the object itself when Symbol.iterator is absent
function counter(k: number) {
  let i = 0;
  return { next: function () { return i < k ? { done: false, value: "c" + (i++) } : { done: true, value: undefined }; } };
}
log("bareIterator=" + letters("d", [2, 1]).flatMap(function (v: any) { return counter(v); }).toArray().join(","));

// 5) a Map and a Set are iterables like any other
log("setReturn=" + JSON.stringify(letters("e", [1]).flatMap(function () { return new Set(["s1", "s2"]); }).toArray()));
log("mapReturn=" + JSON.stringify(letters("f", [1]).flatMap(function () { return new Map([["k", "v"]]); }).toArray()));

// 6) a PRIMITIVE string is refused, even though it is iterable
pulls.length = 0;
log("primitiveString=" + attempt(function () { return letters("g", ["ab"]).flatMap(function (v: any) { return v; }).toArray(); }));
log("stringClosedSource=" + pulls.join(","));

// 7) a String OBJECT is accepted, because the rule is about primitives
log("stringObject=" + JSON.stringify(letters("h", [1]).flatMap(function () { return new String("hi"); }).toArray()));

// 8) numbers, null, undefined and plain objects are all TypeErrors
pulls.length = 0;
log("numberReturn=" + attempt(function () { return letters("i", [1]).flatMap(function () { return 5; }).toArray(); }));
log("nullReturn=" + attempt(function () { return letters("j", [1]).flatMap(function () { return null; }).toArray(); }));
log("undefinedReturn=" + attempt(function () { return letters("k", [1]).flatMap(function () { return undefined; }).toArray(); }));
log("plainObject=" + attempt(function () { return letters("l", [1]).flatMap(function () { return {}; }).toArray(); }));
log("closedOnEach=" + pulls.join(","));

// 9) a callback that THROWS closes the source and propagates
pulls.length = 0;
log("callbackThrows=" + attempt(function () {
  return letters("m", [1, 2, 3]).flatMap(function (v: any) { if (v === 2) throw new RangeError("stop"); return [v]; }).toArray();
}));
log("throwPulls=" + pulls.join(","));

// 10) the callback receives (value, index) with the index counting SOURCE items
const args: string[] = [];
letters("n", ["u", "v", "w"]).flatMap(function (v: any, i: number) { args.push(v + "@" + i); return [v]; }).toArray();
log("callbackArgs=" + args.join(","));

// 11) flatMap is lazy: nothing is pulled until next(), and one source value at
//     a time
pulls.length = 0;
const lazy = letters("o", ["a", "b"]).flatMap(function (v: any) { return [v + "1", v + "2"]; });
log("beforeFirstNext=" + JSON.stringify(pulls.join(",")));
log("first=" + JSON.stringify(lazy.next()) + " pulls=" + pulls.join(","));
log("second=" + JSON.stringify(lazy.next()) + " pulls=" + pulls.join(","));
log("third=" + JSON.stringify(lazy.next()) + " pulls=" + pulls.join(","));

// 12) closing the flatMap mid-inner-iterable closes the inner one too
const innerMarks: string[] = [];
function* innerTracked(v: any) {
  try { yield v + "-1"; yield v + "-2"; } finally { innerMarks.push("inner-closed:" + v); }
}
const closing: any = letters("p", ["z"]).flatMap(function (v: any) { return innerTracked(v); });
log("closingFirst=" + closing.next().value);
log("closingReturn=" + JSON.stringify(closing.return("bye")));
log("innerMarks=" + innerMarks.join(",") + " sourceClosed=" + (pulls.indexOf("p-closed") >= 0));

// 13) an inner iterable of ITERABLES is only flattened one level
log("oneLevelOnly=" + JSON.stringify(letters("q", [1]).flatMap(function () { return [[1, 2], [3]]; }).toArray()));

console.log("end");
