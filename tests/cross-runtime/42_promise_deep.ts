// Cross-runtime future coverage: Promise chaining and combinators.
async function main() {
  const one = await Promise.resolve(3);
  console.log("one=" + one);

  const two = await Promise.resolve(4).then((n: number) => n + 1);
  console.log("two=" + two);

  const all = await Promise.all([Promise.resolve("a"), Promise.resolve("b")]);
  console.log("all=" + all.join(","));

  async function syncAsync(): Promise<number> {
    return 9;
  }
  console.log("async=" + (await syncAsync()));
}

main();
