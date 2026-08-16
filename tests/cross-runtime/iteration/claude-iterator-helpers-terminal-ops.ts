// Cross-runtime: the TERMINAL iterator helpers -- reduce, toArray, forEach,
// some, every, find. Focus: which of them short-circuit, that a short-circuit
// CLOSES the source, and what reduce does with and without an initial value.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const pulls: string[] = [];

function* nums(tag: string, upto: number) {
  try {
    for (let i = 1; i <= upto; i++) { pulls.push(tag + i); yield i; }
  } finally {
    pulls.push(tag + "-closed");
  }
}

// 1) reduce with an initial value
pulls.length = 0;
log("1 sum=" + nums("a", 4).reduce(function (acc: number, v: number) { return acc + v; }, 100));
log("1 pulls=" + pulls.join(","));

// 2) reduce with NO initial value takes the first element as the seed
pulls.length = 0;
log("2 sum=" + nums("b", 4).reduce(function (acc: number, v: number) { return acc + v; }));
log("2 pulls=" + pulls.join(","));

// 3) reduce over an empty iterator: with a seed it answers the seed, without
//    one it throws
function* none() { }
log("3 withSeed=" + none().reduce(function (a: any, b: any) { return a + b; }, "seed"));
log("3 withoutSeed=" + (function () {
  try { return String(none().reduce(function (a: any, b: any) { return a + b; })); }
  catch (e: any) { return e.constructor.name; }
})());

// 4) the reducer's index argument
const idx: string[] = [];
nums("c", 3).reduce(function (acc: any, v: number, i: number) { idx.push(v + "@" + i); return acc; }, 0);
log("4 indices=" + idx.join(","));

// 5) some short-circuits and closes the source
pulls.length = 0;
log("5 some=" + nums("d", 10).some(function (v: number) { return v === 3; }));
log("5 pulls=" + pulls.join(","));

// 6) some that never matches drains the source
pulls.length = 0;
log("6 some=" + nums("e", 3).some(function (v: number) { return v > 99; }));
log("6 pulls=" + pulls.join(","));

// 7) every short-circuits on the first failure
pulls.length = 0;
log("7 every=" + nums("f", 10).every(function (v: number) { return v < 3; }));
log("7 pulls=" + pulls.join(","));

// 8) every over everything true drains it
pulls.length = 0;
log("8 every=" + nums("g", 3).every(function () { return true; }));
log("8 pulls=" + pulls.join(","));

// 9) find returns the value and closes; a miss returns undefined
pulls.length = 0;
log("9 find=" + nums("h", 10).find(function (v: number) { return v % 4 === 0; }));
log("9 pulls=" + pulls.join(","));
pulls.length = 0;
log("9 miss=" + String(nums("i", 3).find(function (v: number) { return v > 99; })));
log("9 pulls=" + pulls.join(","));

// 10) forEach visits everything and answers undefined
pulls.length = 0;
const visited: string[] = [];
const fe = nums("j", 3).forEach(function (v: number, i: number) { visited.push(v + "@" + i); });
log("10 returns=" + String(fe) + " visited=" + visited.join(","));
log("10 pulls=" + pulls.join(","));

// 11) a callback that THROWS closes the source
pulls.length = 0;
log("11 caught=" + (function () {
  try { nums("k", 10).forEach(function (v: number) { if (v === 2) throw new RangeError("stop"); }); return "no"; }
  catch (e: any) { return e.constructor.name; }
})());
log("11 pulls=" + pulls.join(","));

// 12) the same, one helper deeper: map's callback throwing closes the source
pulls.length = 0;
log("12 caught=" + (function () {
  try { nums("m", 10).map(function (v: number) { if (v === 2) throw new TypeError("stop"); return v; }).toArray(); return "no"; }
  catch (e: any) { return e.constructor.name; }
})());
log("12 pulls=" + pulls.join(","));

// 13) toArray on an empty source
log("13 empty=" + JSON.stringify(none().toArray()));

// 14) every terminal op leaves the iterator done
const it14 = nums("p", 3);
it14.toArray();
log("14 afterToArray=" + JSON.stringify(it14.next()));

// 15) a non-callable argument is refused before anything is pulled
pulls.length = 0;
log("15 badCallback=" + (function () {
  try { nums("q", 3).forEach(42 as any); return "no"; } catch (e: any) { return e.constructor.name; }
})());
log("15 pulls=" + JSON.stringify(pulls.join(",")));

console.log("end");
