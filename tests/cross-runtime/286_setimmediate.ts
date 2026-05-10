// Cross-runtime: setImmediate ordering.
const out: string[] = [];
setTimeout(() => out.push("timeout"), 0);
setImmediate(() => out.push("immediate"));
queueMicrotask(() => out.push("micro"));
setTimeout(() => console.log("order=" + out.join("|")), 10);
