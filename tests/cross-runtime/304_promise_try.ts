// Cross-runtime: Promise.try via native or local fallback.
const pTry = (fn: () => any) =>
  Promise.try ? Promise.try.call(Promise, fn) : Promise.resolve().then(fn);
console.log("ok=" + (await pTry(() => 5)));
let err = "";
try { await pTry(() => { throw new Error("boom"); }); } catch (e: any) { err = e.message; }
console.log("err=" + err);
