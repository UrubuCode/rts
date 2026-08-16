// Cross-runtime: the OBJECT `Promise.withResolvers()` hands back -- its own key
// order, its prototype, the arity of the two functions, and that they are the
// very functions the executor of `new Promise` would have received.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const kit: any = Promise.withResolvers();

// 1) a plain object with exactly three own enumerable keys. Their ORDER is not
//    asserted: Bun lists resolve,reject,promise and Node promise,resolve,reject,
//    so the set is the only comparable claim.
log("keys=" + Object.keys(kit).sort().join(","));
log("ownNames=" + Object.getOwnPropertyNames(kit).sort().join(","));
log("protoIsObject=" + (Object.getPrototypeOf(kit) === Object.prototype));
log("noSymbols=" + (Object.getOwnPropertySymbols(kit).length === 0));

// 2) every own property is a plain data property, writable and configurable
const d = Object.getOwnPropertyDescriptor(kit, "promise") as any;
log("promiseDescriptor=" + [d.writable, d.enumerable, d.configurable, typeof d.value].join(","));

// 3) the parts
log("promiseIsPromise=" + (kit.promise instanceof Promise));
log("types=" + typeof kit.resolve + "," + typeof kit.reject);
log("arity=" + kit.resolve.length + "," + kit.reject.length);
log("protoOfResolve=" + (Object.getPrototypeOf(kit.resolve) === Function.prototype));

// 4) the same functions an executor gets: neither is a method of the promise
let exResolve: any, exReject: any;
const viaExecutor = new Promise(function (res, rej) { exResolve = res; exReject = rej; });
log("executorArity=" + exResolve.length + "," + exReject.length);
log("notOnPromise=" + (("resolve" in kit.promise) === false));
log("distinctPerCall=" + (Promise.withResolvers().resolve !== Promise.withResolvers().resolve));

// 5) called on a SUBCLASS the promise is of that subclass
class MyP extends Promise<any> { }
const subKit: any = (Promise.withResolvers as any).call(MyP);
log("subclassPromise=" + (subKit.promise instanceof MyP) + "," + (subKit.promise.constructor === MyP));
log("subclassKeys=" + Object.keys(subKit).sort().join(","));

// 6) `this` must be a constructor
log("badThis=" + (function () {
  try { (Promise.withResolvers as any).call({}); return "no"; } catch (e: any) { return e.constructor.name; }
})());

// 7) resolve settles once; a later reject is a no-op
kit.resolve("first");
kit.reject("ignored");
kit.resolve("also-ignored");

// 8) the reject side of a second kit, settled before anyone listens
const kit2: any = Promise.withResolvers();
kit2.reject("why");
kit2.resolve("too-late");
kit2.promise.catch(function () { });

// 9) resolve/reject work detached from the object
const kit3: any = Promise.withResolvers();
const detached = kit3.resolve;
detached("detached-ok");

viaExecutor.catch(function () { });
exReject("executor-rejected");

(async function () {
  log("kitValue=" + (await kit.promise));
  log("kit2Reason=" + (await kit2.promise.then(function () { return "fulfilled"; }, function (e: any) { return "rejected:" + e; })));
  log("kit3Value=" + (await kit3.promise));
  log("executorValue=" + (await viaExecutor.then(function (v: any) { return "fulfilled:" + v; }, function (e: any) { return "rejected:" + e; })));
  log("subclassThenType=" + (subKit.promise.then(function () { }) instanceof MyP));
  subKit.resolve(1);
  log("subclassValue=" + (await subKit.promise));
  log("methodLength=" + (Promise.withResolvers as any).length + " name=" + (Promise.withResolvers as any).name);
  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
