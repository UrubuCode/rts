// Cross-runtime: queueMicrotask order vs Promise.then and timeout.
const out: string[] = [];
out.push("sync");
queueMicrotask(() => out.push("qm"));
Promise.resolve().then(() => out.push("then"));
setTimeout(() => console.log("order=" + out.concat("timeout").join("|")), 0);
