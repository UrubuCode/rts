// Cross-runtime: the `new Promise` executor -- called synchronously, its
// resolve/reject functions settle exactly ONCE, and a throw after a settlement
// is swallowed. Focus: the single-settlement rule and the function shapes.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

// 1) the executor runs synchronously, before the constructor returns
let ran = "no";
const p1 = new Promise(function (r) { ran = "yes"; r("a"); });
log("executorRanSynchronously=" + ran);

// 2) the resolve/reject functions: shape and identity
let res2: any; let rej2: any;
const p2 = new Promise(function (r, j) { res2 = r; rej2 = j; });
log("resolveType=" + typeof res2 + " rejectType=" + typeof rej2);
log("resolveLength=" + res2.length + " rejectLength=" + rej2.length);
log("resolveName=" + JSON.stringify(res2.name) + " rejectName=" + JSON.stringify(rej2.name));
log("distinctFns=" + (res2 !== rej2));
log("noPrototype=" + (res2.prototype === undefined));

// 3) resolve twice: the second call is ignored
let res3: any;
const p3 = new Promise(function (r) { r("first"); r("second"); });
p3.then(function (v: any) { log("resolveTwice=" + v); });

// 4) resolve then reject: the reject is ignored
const p4 = new Promise(function (r, j) { r("won"); j(new RangeError("lost")); });
p4.then(
  function (v: any) { log("resolveThenReject fulfilled " + v); },
  function (e: any) { log("resolveThenReject rejected " + e.constructor.name); }
);

// 5) reject then resolve: the resolve is ignored
const p5 = new Promise(function (r, j) { j(new EvalError("won")); r("lost"); });
p5.then(
  function (v: any) { log("rejectThenResolve fulfilled " + v); },
  function (e: any) { log("rejectThenResolve rejected " + e.constructor.name); }
);

// 6) a throw AFTER resolving is swallowed, not turned into a rejection
const p6 = new Promise(function (r) { r("kept"); throw new URIError("ignored"); });
p6.then(
  function (v: any) { log("throwAfterResolve fulfilled " + v); },
  function (e: any) { log("throwAfterResolve rejected " + e.constructor.name); }
);

// 7) a throw BEFORE settling rejects the promise
const p7 = new Promise(function () { throw new TypeError("boom"); });
p7.then(
  function () { log("throwFirst fulfilled"); },
  function (e: any) { log("throwFirst rejected " + e.constructor.name); }
);

// 8) resolving LATE, from outside the executor, works exactly once
res2("late");
rej2(new RangeError("too late"));
p2.then(
  function (v: any) { log("late fulfilled " + v); },
  function (e: any) { log("late rejected " + e.constructor.name); }
);

// 9) a non-callable executor throws synchronously at construction
let threw9 = "no";
try { new (Promise as any)(42); } catch (e: any) { threw9 = e.constructor.name; }
log("nonCallableExecutor=" + threw9);

// 10) calling Promise without new
let threw10 = "no";
try { (Promise as any)(function () { }); } catch (e: any) { threw10 = e.constructor.name; }
log("calledWithoutNew=" + threw10);

// 11) resolving with a thenable that settles twice: only the first counts
const p11 = new Promise(function (r) {
  r({ then: function (rr: any, jj: any) { rr("one"); jj(new RangeError("two")); rr("three"); } });
});
p11.then(
  function (v: any) { log("thenableTwice fulfilled " + v); },
  function (e: any) { log("thenableTwice rejected " + e.constructor.name); }
);

// 12) drain and finish
Promise.all([p1, p3, p4, p6]).then(function () {
  let tail: Promise<any> = Promise.resolve();
  for (let i = 0; i < 6; i++) tail = tail.then(function () { return undefined; });
  return tail;
}).then(function () { console.log("end"); });
