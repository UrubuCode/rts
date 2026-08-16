// Cross-runtime: Promise.all preserves input order across adversarial thenable timing.
const seen: string[] = [];
const slow = { then(resolve: any) { seen.push("slow-then"); Promise.resolve().then(() => resolve("S")); } };
const fast = { then(resolve: any) { seen.push("fast-then"); resolve("F"); } };
Promise.all([slow, fast, Promise.resolve("P")]).then((values) => {
  seen.push("result:" + values.join(","));
  console.log(seen.join("|"));
});

