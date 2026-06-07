// Cross-runtime: reentrant then scheduling order.
const log: string[] = [];
const p = Promise.resolve();
p.then(() => {
  log.push("a");
  p.then(() => log.push("c"));
});
p.then(() => log.push("b"));
Promise.resolve().then(() => log.push("d"));
setTimeout(() => console.log(log.join(",")), 0);
