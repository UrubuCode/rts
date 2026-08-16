// Cross-runtime: returning an object from Symbol.toPrimitive is a TypeError.
const value = { [Symbol.toPrimitive]() { return {}; } };
const results: boolean[] = [];
for (const op of [() => +value, () => String(value), () => value == 1]) {
  try { op(); results.push(false); } catch (e) { results.push(e instanceof TypeError); }
}
console.log(results.join(","));

