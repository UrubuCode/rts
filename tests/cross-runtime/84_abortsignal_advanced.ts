// Cross-runtime compatibility: AbortSignal helpers.
const aborted = AbortSignal.abort("x");
console.log("ab=" + aborted.aborted + ":" + aborted.reason);

const timed = AbortSignal.timeout(1);
await new Promise((resolve) => setTimeout(resolve, 10));
console.log("to=" + timed.aborted + ":" + timed.reason.name);
