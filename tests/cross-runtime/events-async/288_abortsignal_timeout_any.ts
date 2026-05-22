// Cross-runtime: AbortSignal.timeout and AbortSignal.any.
const a = AbortSignal.abort("a");
const b = AbortSignal.timeout(1);
const c = AbortSignal.any([a, b]);
await new Promise((r) => setTimeout(r, 10));
console.log("a=" + a.aborted + ":" + a.reason);
console.log("c=" + c.aborted + ":" + c.reason);
