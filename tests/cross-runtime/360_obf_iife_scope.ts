// Cross-runtime: IIFE scope isolation + variable mangling (esbuild/terser output).

// Terser output: all vars renamed to single chars, IIFEs for scope
const _result = (function(_a: number, _b: number, _c: number) {
  var _d = _a + _b;
  var _e = _d * _c;
  var _f = function(_g: number) { return _g > 0 ? _g : -_g; };
  return _f(_e - 100);
})(3, 4, 10);
console.log(_result);  // |70-100| = 30

// Nested IIFE module pattern (webpack legacy output)
var _module = (function() {
  var _private = 0;
  function _inc() { _private++; }
  function _dec() { _private--; }
  function _get() { return _private; }
  return { inc: _inc, dec: _dec, get: _get };
})();
_module.inc(); _module.inc(); _module.inc(); _module.dec();
console.log(_module.get());  // 2

// Chained IIFE with argument passing (UMD pattern)
;(function(_root: any, _factory: () => any) {
  _root._lib = _factory();
})(globalThis as any, function() {
  const _cache = new Map<string, number>();
  return {
    compute(key: string, val: number) {
      if (!_cache.has(key)) _cache.set(key, val * val);
      return _cache.get(key)!;
    }
  };
});
const _lib = (globalThis as any)._lib;
console.log(_lib.compute("x", 7));    // 49
console.log(_lib.compute("x", 99));   // still 49 (cached)
console.log(_lib.compute("y", 3));    // 9

// Minified ternary chains (terser loves these)
function _classify(n: number): string {
  return n < 0 ? "neg" : n === 0 ? "zero" : n < 10 ? "small" : n < 100 ? "medium" : "large";
}
console.log(_classify(-5));
console.log(_classify(0));
console.log(_classify(7));
console.log(_classify(42));
console.log(_classify(999));
