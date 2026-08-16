// Cross-runtime: Promise.race assimilates inputs in iteration order.
const seen: string[] = [];
const first = { then(resolve: any) { seen.push("first"); resolve("A"); } };
const second = { then(resolve: any) { seen.push("second"); resolve("B"); } };
Promise.race([first, second]).then((value) => {
  seen.push("winner:" + value);
  console.log(seen.join("|"));
});

