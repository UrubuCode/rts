// Cross-runtime: Array.fromAsync on async iterables and promises.
async function* gen() { yield 1; yield 2; }
console.log("gen=" + (await Array.fromAsync(gen(), (x) => x * 3)).join(","));
console.log("prom=" + (await Array.fromAsync([Promise.resolve("a"), Promise.resolve("b")])).join(","));
