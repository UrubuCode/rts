// Cross-runtime: resolving a promise WITH ITSELF is a TypeError ("chaining
// cycle"), delivered as a rejection rather than a throw. Focus: the cycle check
// fires for the resolve function only, and a two-promise cycle is NOT caught.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

// 1) the classic cycle: the executor's resolve is called with its own promise
let res1: any;
const p1 = new Promise(function (r) { res1 = r; });
const threw1 = (function () { try { res1(p1); return "no"; } catch (e: any) { return e.name; } })();
log("resolveThrewSynchronously=" + threw1);
p1.then(
  function () { log("p1 fulfilled"); },
  function (e: any) {
    log("p1 rejected " + e.constructor.name);
    log("p1 isTypeError=" + (e instanceof TypeError));
    log("p1 isError=" + (e instanceof Error));
    log("p1 tag=" + Object.prototype.toString.call(e));
    log("p1 messageIsString=" + (typeof e.message === "string"));
  }
);

// 2) withResolvers-shaped cycle, same rule
const kit: any = (Promise as any).withResolvers();
kit.resolve(kit.promise);
kit.promise.then(
  function () { log("p2 fulfilled"); },
  function (e: any) { log("p2 rejected " + e.name); }
);

// 3) REJECTING with the promise itself is fine -- no cycle check on that path
let rej3: any;
const p3 = new Promise(function (_r, j) { rej3 = j; });
rej3(p3);
p3.then(
  function () { log("p3 fulfilled"); },
  function (e: any) { log("p3 rejected withSelf=" + (e === p3)); }
);

// 4) a two-promise cycle is NOT caught: it simply stays forever pending
let ra: any; let rb: any;
const pa = new Promise(function (r) { ra = r; });
const pb = new Promise(function (r) { rb = r; });
ra(pb);
rb(pa);
let mutualSettled = false;
pa.then(function () { mutualSettled = true; }, function () { mutualSettled = true; });

// 5) resolve-with-self AFTER an ordinary resolve is ignored (already settled)
let res5: any;
const p5 = new Promise(function (r) { res5 = r; });
res5("first");
res5(p5);
p5.then(
  function (v: any) { log("p5 fulfilled " + v); },
  function (e: any) { log("p5 rejected " + e.name); }
);

// 6) the cycle rejection is a FRESH TypeError each time, not a shared singleton
let e6a: any; let e6b: any;
let r6a: any; let r6b: any;
const p6a = new Promise(function (r) { r6a = r; });
const p6b = new Promise(function (r) { r6b = r; });
r6a(p6a);
r6b(p6b);
p6a.catch(function (e: any) { e6a = e; });
p6b.catch(function (e: any) { e6b = e; });

// 7) a cycled promise inside Promise.all rejects the whole thing
let res7: any;
const p7 = new Promise(function (r) { res7 = r; });
res7(p7);
Promise.all([Promise.resolve("a"), p7, Promise.resolve("c")]).then(
  function () { log("all fulfilled"); },
  function (e: any) { log("all rejected " + e.name); }
);

// 8) a cycle through a thenable that hands the promise back is NOT a cycle:
//    only the resolve-with-own-promise identity check exists
let res8: any;
const p8 = new Promise(function (r) { res8 = r; });
res8({ then: function (r: any) { r("viaThenable"); } });
p8.then(function (v: any) { log("p8 fulfilled " + v); });

// 9) report the late observations from the tail of the microtask queue
let tail: Promise<any> = Promise.resolve();
for (let i = 0; i < 14; i++) tail = tail.then(function () { return undefined; });
tail.then(function () {
  log("mutualSettled=" + mutualSettled);
  log("distinctErrors=" + (e6a !== e6b));
  log("bothTypeError=" + ((e6a instanceof TypeError) && (e6b instanceof TypeError)));
  log("errorName=" + e6a.name);
  log("errorOwnStack=" + (typeof e6a.stack === "string"));
  console.log("end");
});
