// Cross-runtime: array callback thisArg and source mutation windows.
const ctx = { mul: 3 };
const a = [1, 2, 3, 4];
const mapped = a.map(function (this: any, v, i, src) {
  if (i === 0) src.push(99);
  if (i === 1) src[3] = 40;
  return v * this.mul;
}, ctx);

console.log(mapped.join(","));
console.log(a.join(","));
console.log(a.filter(function (this: any, v) { return v < this.limit; }, { limit: 10 }).join(","));
