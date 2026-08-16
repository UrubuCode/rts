// Cross-runtime: iterator next must return an object result.
const iterable = {
  [Symbol.iterator]() {
    return { next() { return 1 as any; } };
  },
};
const results: boolean[] = [];
try { [...iterable]; } catch (e) { results.push(e instanceof TypeError); }
try { for (const x of iterable) void x; } catch (e) { results.push(e instanceof TypeError); }
console.log(results.join(","));

