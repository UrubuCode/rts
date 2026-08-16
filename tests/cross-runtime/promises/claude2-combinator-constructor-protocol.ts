// Cross-runtime: the combinators are GENERIC over `this` -- each builds its
// result with `new this(executor)` and adopts every entry through `this.resolve`,
// which is READ once per call and then CALLED once per entry.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

function makeCtor() {
  const st: any = { constructs: 0, reads: 0, calls: 0, status: "pending", value: undefined };
  function C(this: any, ex: any) {
    st.constructs++;
    ex(
      function (v: any) { if (st.status === "pending") { st.status = "fulfilled"; st.value = v; } },
      function (e: any) { if (st.status === "pending") { st.status = "rejected"; st.value = e; } }
    );
  }
  Object.defineProperty(C, "resolve", {
    configurable: true,
    get: function () {
      st.reads++;
      return function (v: any) { st.calls++; return Promise.resolve(v); };
    }
  });
  return { C: C as any, st: st };
}

function shape(st: any): string {
  return "constructs=" + st.constructs + " reads=" + st.reads + " calls=" + st.calls;
}

// 1) Promise.all on a foreign constructor: one construct, one read, one call
//    per entry, and the instance is of that constructor.
const a = makeCtor();
const rAll = (Promise.all as any).call(a.C, [1, 2, 3]);
log("all isInstance=" + (rAll instanceof a.C));
log("all " + shape(a.st));
log("all isNativePromise=" + (rAll instanceof Promise));

// 2) race, allSettled and any read `resolve` exactly once as well
const b = makeCtor();
(Promise.race as any).call(b.C, [1, 2]);
log("race " + shape(b.st));

const c = makeCtor();
(Promise.allSettled as any).call(c.C, [1, 2, 3, 4]);
log("allSettled " + shape(c.st));

const d = makeCtor();
(Promise.any as any).call(d.C, [1, 2]);
log("any " + shape(d.st));

// 3) an EMPTY iterable still constructs the capability and still reads resolve
const e = makeCtor();
(Promise.all as any).call(e.C, []);
log("emptyAll " + shape(e.st) + " status=" + e.st.status + " value=" + JSON.stringify(e.st.value));

// 4) Promise.resolve / Promise.reject on a foreign constructor build the same
//    capability but never touch `this.resolve`
const f = makeCtor();
const rf = (Promise.resolve as any).call(f.C, 7);
log("resolveOnForeign isInstance=" + (rf instanceof f.C) + " " + shape(f.st) + " status=" + f.st.status + " value=" + f.st.value);

const g = makeCtor();
const rg = (Promise.reject as any).call(g.C, "boom");
log("rejectOnForeign isInstance=" + (rg instanceof g.C) + " " + shape(g.st) + " status=" + g.st.status + " value=" + g.st.value);

// 5) a non-callable `resolve` rejects the capability instead of throwing
const h = makeCtor();
Object.defineProperty(h.C, "resolve", { configurable: true, value: 42 });
let threw = "no";
try { (Promise.all as any).call(h.C, [1]); } catch (err: any) { threw = err.constructor.name; }
log("badResolve threw=" + threw + " status=" + h.st.status + " value=" + (h.st.value === undefined ? "undefined" : h.st.value.constructor.name));

// 6) a `this` that is not a constructor throws SYNCHRONOUSLY
log("nonCtor=" + (function () {
  try { (Promise.all as any).call({}, [1]); return "no"; } catch (err: any) { return err.constructor.name; }
})());

// 7) the per-entry resolve function of Promise.all is SINGLE-SHOT: a thenable
//    that calls it twice contributes one slot, with the first value.
const twice = {
  then: function (res: any) { res("first"); res("second"); }
};
Promise.all([twice, "plain"]).then(function (v: any) {
  log("singleShot=" + JSON.stringify(v));
}).catch(function () { log("singleShot=unexpected"); });

// 8) drain, then report what the foreign capabilities received
(async function () {
  for (let i = 0; i < 12; i++) await null;
  log("allStatus=" + a.st.status + " value=" + JSON.stringify(a.st.value));
  log("raceStatus=" + b.st.status + " value=" + JSON.stringify(b.st.value));
  log("allSettledStatus=" + c.st.status + " len=" + (c.st.value as any).length + " first=" + (c.st.value as any)[0].status);
  log("anyStatus=" + d.st.status + " value=" + JSON.stringify(d.st.value));
  log("finalCalls all=" + a.st.calls + " race=" + b.st.calls + " allSettled=" + c.st.calls + " any=" + d.st.calls);
  console.log("end");
})();
