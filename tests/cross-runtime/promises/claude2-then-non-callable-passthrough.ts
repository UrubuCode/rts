// Cross-runtime: `then` with NON-CALLABLE handlers is a pass-through -- the
// settlement travels to the next promise unchanged -- and two `then` calls on
// one promise queue both reactions in REGISTRATION order.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const order: string[] = [];

// 1) every non-callable onFulfilled is ignored and the value passes through
const base = Promise.resolve("V");
const forms: any[] = [undefined, null, 42, "str", true, {}, Symbol.iterator];
const names = ["undefined", "null", "number", "string", "boolean", "object", "symbol"];

(async function () {
  for (let i = 0; i < forms.length; i++) {
    const v = await base.then(forms[i]);
    log("passThrough " + names[i] + "=" + v);
  }

  // 2) a non-callable onRejected lets the REASON through to the next promise
  const rejected = Promise.reject("R");
  const caught = await rejected.then(function (x: any) { return "fulfilled:" + x; }, null as any)
    .catch(function (e: any) { return "caught:" + e; });
  log("rejectionPassThrough=" + caught);

  // 3) then() with no arguments at all is the identity on both channels
  const idOk = await Promise.resolve("ok").then();
  log("thenNoArgsFulfilled=" + idOk);
  const idBad = await Promise.reject("bad").then().catch(function (e: any) { return "caught:" + e; });
  log("thenNoArgsRejected=" + idBad);

  // 4) a fulfilled promise ignores onRejected; a rejected one ignores onFulfilled
  let seen = "none";
  await Promise.resolve(1).then(function () { seen = "onFulfilled"; }, function () { seen = "onRejected"; });
  log("fulfilledPicks=" + seen);
  await Promise.reject(1).then(function () { seen = "onFulfilled"; }, function () { seen = "onRejected"; });
  log("rejectedPicks=" + seen);

  // 5) a handler that returns nothing produces undefined, not the input
  const undef = await Promise.resolve("in").then(function () { });
  log("emptyHandler=" + String(undef));

  // 6) `catch(f)` is exactly `then(undefined, f)`
  log("catchIsThen=" + (Promise.prototype.catch.length === 1 && Promise.prototype.then.length === 2));
  const viaCatch = await Promise.reject("c").catch(function (e: any) { return "byCatch:" + e; });
  const viaThen = await Promise.reject("c").then(undefined, function (e: any) { return "byThen:" + e; });
  log("catchEquivalence=" + viaCatch + "/" + viaThen);

  // 7) every then call answers a DISTINCT new promise
  const p = Promise.resolve("d");
  const t1 = p.then();
  const t2 = p.then();
  log("distinctResults=" + (t1 !== t2) + " neitherIsSource=" + (t1 !== p && t2 !== p));

  // 8) two reactions on ONE promise run in registration order, and interleave
  //    with a second promise's reactions by registration time, not by promise
  order.length = 0;
  const pa = Promise.resolve("a");
  const pb = Promise.resolve("b");
  pa.then(function () { order.push("a1"); });
  pb.then(function () { order.push("b1"); });
  pa.then(function () { order.push("a2"); });
  pb.then(function () { order.push("b2"); });
  pa.then(function () { order.push("a3"); });
  for (let i = 0; i < 4; i++) await null;
  log("registrationOrder=" + order.join(","));

  // 9) a reaction registered on an ALREADY-settled promise still defers one
  //    tick -- it never runs synchronously
  order.length = 0;
  order.push("before");
  Promise.resolve().then(function () { order.push("reaction"); });
  order.push("after");
  log("neverSynchronous=" + order.join(","));
  for (let i = 0; i < 2; i++) await null;
  log("afterDrain=" + order.join(","));

  // 10) a reaction added from INSIDE a reaction of the same promise joins the
  //     tail of the queue
  order.length = 0;
  const pc = Promise.resolve("c");
  pc.then(function () { order.push("r1"); pc.then(function () { order.push("nested"); }); });
  pc.then(function () { order.push("r2"); });
  for (let i = 0; i < 4; i++) await null;
  log("nestedRegistration=" + order.join(","));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
