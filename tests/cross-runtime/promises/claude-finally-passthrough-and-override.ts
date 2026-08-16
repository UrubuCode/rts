// Cross-runtime: `finally` is TRANSPARENT -- its callback's return value is
// discarded and the settlement passes through -- except when the callback
// throws or returns a rejecting thenable, which REPLACES the settlement.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

function report(tag: string, p: Promise<any>) {
  return p.then(
    function (v: any) { log(tag + " fulfilled " + String(v)); },
    function (e: any) { log(tag + " rejected " + (e && e.constructor ? e.constructor.name : String(e))); }
  );
}

// 1) a returned value is thrown away; the original value survives
const a = report("returnValue", Promise.resolve("keep").finally(function () { return "discarded"; }));

// 2) the callback gets NO arguments at all
const b = a.then(function () {
  return report("noArgs", Promise.resolve("v").finally(function () {
    log("argCount=" + arguments.length);
    return undefined;
  }));
});

// 3) a rejection passes straight through an ordinary finally
const c = b.then(function () {
  return report("rejectPass", Promise.reject(new RangeError("r")).finally(function () { return "x"; }));
});

// 4) a throw inside finally REPLACES the fulfilment
const d = c.then(function () {
  return report("throwOverFulfil", Promise.resolve("gone").finally(function () { throw new TypeError("t"); }));
});

// 5) a throw inside finally also replaces an existing REJECTION
const e = d.then(function () {
  return report("throwOverReject", Promise.reject(new RangeError("old")).finally(function () { throw new EvalError("new"); }));
});

// 6) returning a REJECTED promise replaces the settlement too
const f = e.then(function () {
  return report("returnRejected", Promise.resolve("gone").finally(function () { return Promise.reject(new URIError("u")); }));
});

// 7) returning a PENDING promise delays the pass-through; the value is intact
const g = f.then(function () {
  const marks: string[] = [];
  let deep: Promise<any> = Promise.resolve();
  for (let i = 0; i < 5; i++) deep = deep.then(function () { marks.push("tick"); });
  return Promise.resolve("delayed")
    .finally(function () { marks.push("finallyCalled"); return deep; })
    .then(function (v: any) {
      log("delayed value=" + v);
      log("delayed marks=" + marks.join(","));
    });
});

// 8) finally on a non-callable argument is a no-op pass-through
const h = g.then(function () {
  return report("nonCallable", (Promise.resolve("still").finally(null as any)));
});
const i = h.then(function () {
  return report("nonCallableReject", (Promise.reject(new RangeError("q")).finally(42 as any)));
});

// 9) a returned FULFILLING thenable still discards its value
const j = i.then(function () {
  return report("thenableValue", Promise.resolve("original").finally(function () {
    return { then: function (r: any) { r("ignored"); } };
  }));
});

// 10) the finally callback runs before anything registered after it
const k = j.then(function () {
  const seq: string[] = [];
  const src = Promise.resolve("s");
  const chained = src.finally(function () { seq.push("finally"); }).then(function () { seq.push("afterFinally"); });
  const sibling = src.then(function () { seq.push("sibling"); });
  return Promise.all([chained, sibling]).then(function () { log("order=" + seq.join(",")); });
});

// 11) the method's own shape
k.then(function () {
  log("finallyLength=" + Promise.prototype.finally.length);
  log("finallyName=" + Promise.prototype.finally.name);
  log("onPrototype=" + Object.prototype.hasOwnProperty.call(Promise.prototype, "finally"));
});

// 12) finally returns a NEW promise, never the receiver
k.then(function () {
  const src = Promise.resolve(1);
  const out = src.finally(function () { return undefined; });
  log("newPromise=" + (out !== src));
  log("isPromise=" + (out instanceof Promise));
  return out;
}).then(function () {
  console.log("end");
});
