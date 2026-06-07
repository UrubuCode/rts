// Cross-runtime: async generator yield/await/finally sequencing.
async function* gen() {
  try {
    yield await Promise.resolve("a");
    yield "b";
  } finally {
    yield "fin";
  }
}

(async () => {
  const it = gen();
  console.log(JSON.stringify(await it.next()));
  console.log(JSON.stringify(await it.return("ret")));
  console.log(JSON.stringify(await it.next()));
})();
