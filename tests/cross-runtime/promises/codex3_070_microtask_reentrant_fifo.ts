// Cross-runtime: microtasks enqueued reentrantly join the FIFO tail.
const seen: string[] = [];
Promise.resolve().then(() => {
  seen.push("a");
  Promise.resolve().then(() => seen.push("c"));
});
Promise.resolve().then(() => {
  seen.push("b");
  Promise.resolve().then(() => seen.push("d"));
});
Promise.resolve().then(() => Promise.resolve()).then(() => Promise.resolve()).then(() => {
  console.log(seen.join(","));
});

