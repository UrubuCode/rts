// Cross-runtime: an async generator's SHAPE -- its three-level prototype chain,
// Symbol.asyncIterator returning itself, next() answering a promise -- and the
// fact that a yielded promise is AWAITED before it reaches the consumer.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

async function* ag() { yield 1; yield 2; }
const a = ag();

// 1) next() answers a promise, not a result object
const firstCall = a.next();
log("nextIsPromise=" + (firstCall instanceof Promise));
log("selfAsyncIterable=" + ((a as any)[Symbol.asyncIterator]() === a));
log("noSyncIterator=" + ((a as any)[Symbol.iterator] === undefined));

// 2) the prototype chain: object -> fn.prototype -> %AsyncGeneratorPrototype%
//    -> %AsyncIteratorPrototype%
const own = Object.getPrototypeOf(a);
const agProto = Object.getPrototypeOf(own);
const asyncIterProto = Object.getPrototypeOf(agProto);
log("ownIsFnPrototype=" + (own === (ag as any).prototype));
log("agProtoOwns=" + ["next", "throw", "return"].map(function (k) {
  return k + ":" + Object.prototype.hasOwnProperty.call(agProto, k);
}).join(","));
log("asyncIterProtoOwnsSymbol=" + Object.prototype.hasOwnProperty.call(asyncIterProto, Symbol.asyncIterator));
log("asyncIterProtoParent=" + (Object.getPrototypeOf(asyncIterProto) === Object.prototype));
log("tag=" + agProto[Symbol.toStringTag] + " toString=" + Object.prototype.toString.call(a));

// 3) the function object itself
log("fnTag=" + Object.getPrototypeOf(ag)[Symbol.toStringTag]);
log("fnToString=" + Object.prototype.toString.call(ag));
log("fnNotConstructable=" + (function () {
  try { new (ag as any)(); return "no"; } catch (e: any) { return e.constructor.name; }
})());
async function* other() { yield 0; }
log("sharedFnProto=" + (Object.getPrototypeOf(ag) === Object.getPrototypeOf(other)));
log("sharedGenProto=" + (Object.getPrototypeOf((ag as any).prototype) === Object.getPrototypeOf((other as any).prototype)));

// 4) an async generator method on a class and on an object literal
class Src { async *stream() { yield "s1"; yield "s2"; } }
const lit = { async *gen() { yield "l1"; } };
log("classMethodTag=" + Object.prototype.toString.call(new Src().stream()));
log("literalMethodName=" + (lit as any).gen.name + " classMethodName=" + Src.prototype.stream.name);

(async function () {
  // 5) a YIELDED promise is awaited: the consumer sees the value
  async function* awaits() {
    yield Promise.resolve("resolved");
    yield { then: function (r: any) { r("thenable"); } };
    yield "plain";
  }
  const seen: string[] = [];
  for await (const v of awaits()) seen.push(String(v));
  log("yieldedPromisesUnwrapped=" + seen.join(","));

  // 6) the result objects: shape and freshness
  const b = ag();
  const r1 = await b.next();
  log("resultShape=" + JSON.stringify(r1) + " keys=" + Object.keys(r1).sort().join(","));
  log("resultProto=" + (Object.getPrototypeOf(r1) === Object.prototype));
  const r2 = await b.next();
  const r3 = await b.next();
  log("sequence=" + [JSON.stringify(r1), JSON.stringify(r2), JSON.stringify(r3)].join(""));
  log("pastDone=" + JSON.stringify(await b.next()));

  // 7) the first next() from step 1 still resolves, and to the first value
  log("firstCallValue=" + JSON.stringify(await firstCall));

  // 8) `yield*` over a SYNC iterable inside an async generator
  async function* delegating() { yield* ["d1", "d2"]; yield "own"; }
  const got8: string[] = [];
  for await (const v of delegating()) got8.push(String(v));
  log("delegatesSyncIterable=" + got8.join(","));

  // 9) return() answers a promise of a done result and runs the finally
  const marks: string[] = [];
  async function* withFinally() {
    try { yield "f1"; yield "f2"; } finally { marks.push("finally"); }
  }
  const c = withFinally();
  log("cFirst=" + JSON.stringify(await c.next()));
  log("cReturn=" + JSON.stringify(await c.return("R" as any)));
  log("cMarks=" + marks.join(",") + " after=" + JSON.stringify(await c.next()));

  // 10) throw() rejects the promise it answers when the body does not catch
  const d = ag();
  await d.next();
  const outcome = await d.throw(new Error("x")).then(function () { return "fulfilled"; }, function (e: any) { return "rejected:" + e.constructor.name; });
  log("throwRejects=" + outcome + " afterwards=" + JSON.stringify(await d.next()));

  // 11) a body that catches turns the throw into the next value
  async function* catches() {
    try { yield "c1"; } catch (e: any) { yield "caught:" + String(e); }
    yield "c2";
  }
  const e = catches();
  await e.next();
  log("throwCaught=" + JSON.stringify(await e.throw("T" as any)));
  log("throwCaughtNext=" + JSON.stringify(await e.next()));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
