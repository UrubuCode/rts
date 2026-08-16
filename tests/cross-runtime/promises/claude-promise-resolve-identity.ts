// Cross-runtime: `Promise.resolve(x)` returns x ITSELF only when x is a native
// promise whose `.constructor` is the exact receiver. Focus: the identity rule,
// a subclass on both sides of it, and a non-callable `then`.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const base = Promise.resolve(1);

// 1) same constructor -> the very same object comes back
log("sameObject=" + (Promise.resolve(base) === base));
log("viaCall=" + (Promise.resolve.call(Promise, base) === base));

// 2) an executor that resolves WITH a promise never returns that promise
const wrapped = new Promise(function (r) { r(base); });
log("wrappedIsSame=" + (wrapped === base));
log("wrappedIsPromise=" + (wrapped instanceof Promise));

// 3) a subclass: MyP.resolve keeps identity for its OWN instances only
class MyP extends Promise {}
const sub = MyP.resolve(2) as any;
log("subIsMyP=" + (sub instanceof MyP));
log("subIsPromise=" + (sub instanceof Promise));
log("subCtorName=" + sub.constructor.name);
log("MyPResolveSub=" + (MyP.resolve(sub) === sub));
log("PromiseResolveSub=" + ((Promise.resolve(sub) as any) === sub));
log("MyPResolveBase=" + ((MyP.resolve(base) as any) === base));

// 4) constructor swapped on a plain promise defeats the identity rule
const disguised: any = Promise.resolve(3);
disguised.constructor = MyP;
log("disguisedSame=" + (Promise.resolve(disguised) === disguised));
log("disguisedByMyP=" + ((MyP.resolve(disguised) as any) === disguised));

// 5) a `then` that is not callable is NOT a thenable: it fulfils with the object
const notThenable: any = { then: 42, tag: "plain" };
const r5 = Promise.resolve(notThenable);
log("notThenableSame=" + (r5 === notThenable));
log("notThenableIsPromise=" + (r5 instanceof Promise));

// 6) drain, in a fixed order, and report the values
r5.then(function (v: any) {
  log("notThenableValue=" + v.tag + ":" + v.then);
  return base;
}).then(function (v: any) {
  log("baseValue=" + v);
  return sub;
}).then(function (v: any) {
  log("subValue=" + v);
  return disguised;
}).then(function (v: any) {
  log("disguisedValue=" + v);
  return wrapped;
}).then(function (v: any) {
  log("wrappedValue=" + v);
  console.log("end");
});
