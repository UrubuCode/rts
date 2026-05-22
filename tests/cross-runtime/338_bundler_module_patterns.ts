// Cross-runtime: CommonJS/ESM interop patterns emitted by bundlers.

// Pattern: CJS-style module object (bundler wraps modules in this)
const module_exports: Record<string, any> = {};
(function(exports: Record<string, any>) {
  exports.add = (a: number, b: number) => a + b;
  exports.mul = (a: number, b: number) => a * b;
  exports.PI = 3.14159;
})(module_exports);

console.log(module_exports.add(2, 3));
console.log(module_exports.mul(4, 5));
console.log(module_exports.PI);

// Pattern: namespace object (rollup __toESM output)
function __toESM(mod: any) {
  return Object.assign(Object.create(null), mod, { default: mod });
}
const ns = __toESM({ foo: 1, bar: 2 });
console.log(ns.foo);
console.log(ns.default.foo);

// Pattern: lazy getter for circular dep resolution (webpack/rollup)
const lazy: any = {};
let _loaded = false;
let _value = 0;
Object.defineProperty(lazy, "value", {
  get() {
    if (!_loaded) { _loaded = true; _value = 100; }
    return _value;
  }
});
console.log(lazy.value);
console.log(lazy.value);  // cached

// Pattern: Object.defineProperty for exports (tsc --module commonjs output)
const exports_obj: any = {};
Object.defineProperty(exports_obj, "__esModule", { value: true });
exports_obj.hello = "world";
console.log(exports_obj.__esModule);
console.log(exports_obj.hello);
