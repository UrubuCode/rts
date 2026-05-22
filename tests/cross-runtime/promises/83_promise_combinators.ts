// Cross-runtime compatibility: Promise combinators.
const settled = await Promise.allSettled([Promise.resolve(1), Promise.reject("x")]);
console.log(
  "settled=" +
    settled.map((x) => x.status + ":" + ("value" in x ? x.value : x.reason)).join("|"),
);
console.log("any=" + (await Promise.any([Promise.reject("a"), Promise.resolve("b")])));
