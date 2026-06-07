// Cross-runtime: number formatting around exponential/fixed thresholds.
const nums = [1e21, 1e20, 1e-7, 1e-6, 1000000000000000128, 0.0000012345];
for (const n of nums) {
  console.log(String(n));
}
console.log((1.005).toFixed(2));
console.log((12345.6789).toPrecision(6));
