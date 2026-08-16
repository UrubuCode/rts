// Cross-runtime: `Promise.reject` is NOT the mirror of `Promise.resolve` -- it
// never unwraps. A promise or a thenable handed to it becomes the reason ITSELF,
// and the same holds for the executor's reject function.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const inner = Promise.resolve("in");
const thenable = { then: function (r: any) { r("th"); }, tag: "thenable" };

// 1) rejecting with a promise: the reason is that very promise
const r1 = Promise.reject(inner);
// 2) rejecting with a thenable: the reason is that very object
const r2 = Promise.reject(thenable);
// 3) the executor's reject behaves identically
const r3 = new Promise(function (_res, rej) { rej(inner); });
// 4) while the executor's RESOLVE adopts the same promise
const r4 = new Promise(function (res) { res(inner); });
// 5) and Promise.resolve unwraps it to identity
const r5 = Promise.resolve(inner);

log("rejectResultIsNew=" + (r1 !== inner) + " isPromise=" + (r1 instanceof Promise));
log("resolveKeepsIdentity=" + (r5 === inner));

// 6) a rejected promise used as a REASON is not "handled" by that use -- attach
//    a handler to it so nothing is left unhandled anywhere
const rejectedReason = Promise.reject("deep");
rejectedReason.catch(function () { });
const r6 = Promise.reject(rejectedReason);

// 7) throwing a promise from an async function gives the same shape
async function throwsPromise(): Promise<any> { throw inner; }

// 8) `Promise.reject` on a foreign `this` builds that constructor's promise
let capReason: any = "unset";
function Cap(this: any, ex: any) { ex(function () { }, function (e: any) { capReason = e; }); }
const r8 = (Promise.reject as any).call(Cap, inner);

// A reason that is itself a promise cannot be RETURNED out of a handler: the
// next promise would adopt it and `await` would unwrap it. Every reason below
// therefore travels inside a box, which is not a thenable.
function boxed(p: Promise<any>): Promise<any> {
  return p.then(function (v: any) { return { settled: "fulfilled", value: v }; },
    function (e: any) { return { settled: "rejected", value: e }; });
}

(async function () {
  const b1 = await boxed(r1);
  log("boxed1=" + b1.settled + " reasonIsInnerPromise=" + (b1.value === inner) + " typeof=" + typeof b1.value);

  const b2 = await boxed(r2);
  log("reasonIsThenable=" + (b2.value === thenable) + " tag=" + b2.value.tag);

  const b3 = await boxed(r3);
  log("executorRejectSame=" + (b3.value === inner) + " settled=" + b3.settled);

  log("executorResolveAdopts=" + (await r4) + " notIdentity=" + (r4 !== inner));

  const b6 = await boxed(r6);
  log("reasonIsRejectedPromise=" + (b6.value === rejectedReason));
  const inner6 = await boxed(b6.value);
  log("thatReasonStillRejects=" + inner6.settled + ":" + inner6.value);

  const b7 = await boxed(throwsPromise());
  log("thrownPromiseIsReason=" + (b7.value === inner) + " settled=" + b7.settled);

  log("foreignRejectInstance=" + (r8 instanceof (Cap as any)) + " reasonSame=" + (capReason === inner));

  // 9) a rejection reason travels through `then` without any unwrapping
  const b9 = await boxed(Promise.reject(inner));
  log("reasonSurvivesCatch=" + (b9.value === inner));

  // 10) but RETURNING a promise from a catch handler DOES adopt it
  const adopted = await Promise.reject("x").catch(function () { return inner; });
  log("returnedFromCatchAdopted=" + adopted);

  // 11) and re-throwing a promise from a handler keeps it as the reason
  const b11 = await boxed(Promise.reject("y").catch(function () { throw inner; }));
  log("rethrownIsReason=" + (b11.value === inner) + " settled=" + b11.settled);

  // 12) Promise.reject never reads `then` on its argument
  let reads = 0;
  const watched = { get then() { reads++; return function (r: any) { r(1); }; } };
  const b12 = await boxed(Promise.reject(watched));
  log("rejectNeverReadsThen=" + reads + " sameObject=" + (b12.value === watched));
  log("rejectLength=" + Promise.reject.length + " resolveLength=" + Promise.resolve.length);

  // 13) and Promise.resolve DOES read it, exactly once
  const watched2 = { get then() { reads++; return function (r: any) { r("read"); }; } };
  log("resolveReadsThen=" + (await Promise.resolve(watched2)) + " totalReads=" + reads);

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
