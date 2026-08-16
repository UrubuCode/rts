// Cross-runtime: an async function ALWAYS hands back a fresh native Promise --
// never the promise it returned -- and `return p` costs strictly more ticks
// than `return await p`, measured against a ruler chain.

let n = 0;
function log(s: string): void { console.log((++n) + " " + s); }

const inner = Promise.resolve("I");

async function returnsPromise() { return inner; }
async function returnsAwaited() { return await inner; }
async function returnsValue() { return "V"; }
async function throwsSync(): Promise<any> { throw "S"; }

// 1) identity: the returned promise is new, native, and not the inner one
const rp = returnsPromise();
log("notInner=" + (rp !== inner));
log("isPromise=" + (rp instanceof Promise) + " ctor=" + rp.constructor.name);
log("freshEachCall=" + (returnsPromise() !== returnsPromise()));
log("valueCallIsPromise=" + (returnsValue() instanceof Promise));

// 2) a synchronous throw becomes a rejection, not a throw at the call site
let threw = "no";
let syncResult: any;
try { syncResult = throwsSync(); } catch (e: any) { threw = "threw"; }
log("syncThrow=" + threw + " isPromise=" + (syncResult instanceof Promise));
syncResult.catch(function () { });

// 3) an async method / arrow / class method answer native promises too
const obj = { async m() { return 1; } };
const arrow = async () => 2;
class C { async m() { return 3; } static async s() { return 4; } }
log("methodCtor=" + obj.m().constructor.name + " arrowCtor=" + arrow().constructor.name);
log("classCtor=" + new C().m().constructor.name + " staticCtor=" + C.s().constructor.name);

// 4) even a subclass instance returned from an async function is wrapped in a
//    plain Promise
class MyP extends Promise<any> { }
async function returnsSubclass() { return MyP.resolve("sub"); }
const rs = returnsSubclass();
log("subclassWrapped=" + (rs instanceof MyP) + " isPlain=" + (rs.constructor === Promise));

// 5) the ruler: 12 already-scheduled ticks, each one microtask after the last
let ruler: Promise<void> = Promise.resolve();
const ticks: string[] = [];
for (let i = 1; i <= 12; i++) {
  const k = i;
  ruler = ruler.then(function () { ticks.push("t" + k); });
}

// 6) the two shapes, resolved into the same log
returnsAwaited().then(function () { ticks.push("returnAwait"); });
returnsPromise().then(function () { ticks.push("returnPromise"); });
returnsValue().then(function () { ticks.push("returnValue"); });

// 7) awaiting the SAME already-fulfilled promise twice in a row
(async function () {
  await inner;
  ticks.push("await1");
  await inner;
  ticks.push("await2");
})();

log("synchronousTail");

(async function () {
  for (let i = 0; i < 20; i++) await null;
  log("timeline=" + ticks.join(","));
  log("awaitBeforeReturnPromise=" + (ticks.indexOf("returnAwait") < ticks.indexOf("returnPromise")));
  log("valueFirst=" + (ticks.indexOf("returnValue") < ticks.indexOf("returnAwait")));

  // 8) the settled values themselves
  log("returnPromiseValue=" + (await returnsPromise()));
  log("returnAwaitValue=" + (await returnsAwaited()));
  log("throwsSyncReason=" + (await throwsSync().then(function () { return "fulfilled"; }, function (e: any) { return "rejected:" + e; })));

  // 9) returning a rejected promise rejects the outer one, one route or another
  async function returnsRejected() { return Promise.reject("RJ"); }
  log("returnsRejected=" + (await returnsRejected().then(function () { return "fulfilled"; }, function (e: any) { return "rejected:" + e; })));

  // 10) `await` on a non-thenable object hands the object straight back
  const plain = { tag: "plain" };
  log("awaitPlainObject=" + ((await plain) === plain));

  console.log("end");
})().catch(function () { console.log("UNEXPECTED"); });
