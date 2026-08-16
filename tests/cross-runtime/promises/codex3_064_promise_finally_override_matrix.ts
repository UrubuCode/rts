// Cross-runtime: finally preserves settlement unless callback throws or rejects.
const rows: string[] = [];
Promise.resolve("value")
  .finally(() => "ignored")
  .then((v) => rows.push("keep:" + v));
Promise.reject("reason")
  .finally(() => Promise.resolve("ignored"))
  .catch((e) => rows.push("reason:" + e));
Promise.resolve("value")
  .finally(() => { throw new Error("override"); })
  .catch((e) => rows.push("throw:" + e.message));
Promise.reject("reason")
  .finally(() => Promise.reject("new"))
  .catch((e) => rows.push("reject:" + e));
Promise.allSettled([
  Promise.resolve().then(() => Promise.resolve()),
]).then(() => Promise.resolve()).then(() => console.log(rows.sort().join("|")));

