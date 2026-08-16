// Cross-runtime: a thenable whose `then` MISBEHAVES. Once it has called resolve
// or reject, everything it does afterwards -- calling again, calling the other
// one, throwing -- is discarded. Only a throw BEFORE any call rejects.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

function settle(p: Promise<any>): Promise<string> {
  return p.then(function (v: any) { return "fulfilled:" + String(v); },
    function (e: any) { return "rejected:" + (e && e.constructor === Error ? "Error" : String(e)); });
}

// 1) resolve, then throw -- the throw is swallowed
const resolveThenThrow = { then: function (res: any) { res("A"); throw new Error("late"); } };

// 2) throw with nothing called -- the throw becomes the rejection
const throwOnly = { then: function () { throw "T"; } };

// 3) resolve twice -- the first call wins
const resolveTwice = { then: function (res: any) { res("first"); res("second"); } };

// 4) resolve then reject -- the reject is ignored
const resolveThenReject = { then: function (res: any, rej: any) { res("kept"); rej("dropped"); } };

// 5) reject then resolve -- the resolve is ignored
const rejectThenResolve = { then: function (res: any, rej: any) { rej("kept-reason"); res("dropped"); } };

// 6) calls nothing at all -- forever pending, proven by a drain below
let neverMarked = "pending";
const callsNothing = { then: function () { } };

// 7) `then` is called with the thenable as `this` and with exactly two args
const thisAndArgs: any = {
  seen: "",
  then: function (res: any, rej: any) {
    thisAndArgs.seen = "sameThis=" + (this === thisAndArgs) + " args=" + arguments.length +
      " types=" + typeof res + "," + typeof rej + " arity=" + res.length + "," + rej.length;
    res("done");
  }
};

// 8) resolving with a NESTED thenable assimilates again, one level at a time
const nested = { then: function (res: any) { res({ then: function (r2: any) { r2("inner"); } }); } };

// 9) a thenable that resolves with ITSELF is a cycle only for the resolve
//    function of a real promise; here `then` re-enters and is called again,
//    so the guard is that a promise resolved with a thenable resolving with a
//    promise ends up with the innermost value
const viaPromise = { then: function (res: any) { res(Promise.resolve("unwrapped")); } };

(async function () {
  log("resolveThenThrow=" + await settle(Promise.resolve(resolveThenThrow)));
  log("throwOnly=" + await settle(Promise.resolve(throwOnly)));
  log("resolveTwice=" + await settle(Promise.resolve(resolveTwice)));
  log("resolveThenReject=" + await settle(Promise.resolve(resolveThenReject)));
  log("rejectThenResolve=" + await settle(Promise.resolve(rejectThenResolve)));
  log("nested=" + await settle(Promise.resolve(nested)));
  log("viaPromise=" + await settle(Promise.resolve(viaPromise)));
  log("thisAndArgsValue=" + await settle(Promise.resolve(thisAndArgs)));
  log("thisAndArgs=" + thisAndArgs.seen);

  // the never-calling thenable: still pending after a deep drain
  Promise.resolve(callsNothing).then(function () { neverMarked = "fulfilled"; }, function () { neverMarked = "rejected"; });
  for (let i = 0; i < 20; i++) await null;
  log("callsNothing=" + neverMarked);

  // 10) the same misbehaviour inside `new Promise`: the executor's resolve is
  //     the one that assimilates, and a throw after it is swallowed
  const inExecutor = new Promise(function (res: any) { res(resolveTwice); throw new Error("after"); });
  log("executorThrowAfterResolve=" + await settle(inExecutor));

  // 11) and a throw BEFORE resolving rejects the promise
  const executorThrowFirst = new Promise(function () { throw "first-thing"; });
  log("executorThrowFirst=" + await settle(executorThrowFirst));

  // 12) a thenable is only a thenable when `then` is callable
  const notCallable: any = { then: 1 };
  const asValue = await Promise.resolve(notCallable);
  log("nonCallableThen=" + (asValue === notCallable) + " then=" + asValue.then);

  // 13) `then` inherited from a prototype counts
  const proto = { then: function (res: any) { res("inherited"); } };
  const child = Object.create(proto);
  log("inheritedThen=" + await settle(Promise.resolve(child)));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
