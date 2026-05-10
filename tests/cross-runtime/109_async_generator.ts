// Cross-runtime: async generators (deterministic)
async function* asyncGen() {
  yield 1;
  yield 2;
  yield 3;
}

(async () => {
  const results: number[] = [];
  for await (const val of asyncGen()) {
    results.push(val);
  }
  console.log("async=" + results.join(","));
})();
