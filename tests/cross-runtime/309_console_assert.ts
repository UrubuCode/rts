// Cross-runtime: console.assert does not throw and execution continues.
let threw = false;
try { console.assert(false, "boom"); } catch { threw = true; }
console.assert(true, "skip");
console.log("threw=" + threw);
console.log("after=ok");
