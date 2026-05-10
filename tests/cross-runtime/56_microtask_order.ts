// Cross-runtime compatibility: microtask ordering.
const out: string[] = [];

out.push("sync:start");
queueMicrotask(() => out.push("micro:qm"));
Promise.resolve().then(() => out.push("micro:promise"));
out.push("sync:end");

setTimeout(() => {
  out.push("macro:timeout");
  console.log(out.join("|"));
}, 0);
