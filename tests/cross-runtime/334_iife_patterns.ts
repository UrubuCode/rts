// Cross-runtime: IIFE patterns used by bundlers.

// classic IIFE
const r1 = (function() { return 42; })();
console.log(r1);

// arrow IIFE
const r2 = (() => 99)();
console.log(r2);

// IIFE with params
const r3 = ((a: number, b: number) => a + b)(3, 4);
console.log(r3);

// IIFE for scope isolation (bundler module wrapper)
const result = (function(global: { count: number }) {
  global.count += 1;
  return global.count;
})({ count: 10 });
console.log(result);

// void IIFE (bundler pattern to suppress return value warnings)
void (function init() {
  console.log("init called");
})();

// nested IIFE
const nested = (() => {
  const inner = (() => 7)();
  return inner * 6;
})();
console.log(nested);
