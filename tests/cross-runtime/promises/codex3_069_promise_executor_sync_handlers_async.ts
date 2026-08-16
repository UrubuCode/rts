// Cross-runtime: executor runs synchronously while reactions always run later.
const seen: string[] = [];
const promise = new Promise<number>((resolve) => {
  seen.push("executor");
  resolve(1);
  seen.push("after-resolve");
});
promise.then((v) => seen.push("then:" + v));
seen.push("sync-end");
Promise.resolve().then(() => console.log(seen.join("|")));

