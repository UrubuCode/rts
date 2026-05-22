// Cross-runtime: helper functions emitted by tsc/esbuild/swc/rollup.

// __assign (tsc spread helper)
function __assign(target: any, ...sources: any[]) {
  for (const src of sources) {
    for (const key of Object.keys(src)) target[key] = src[key];
  }
  return target;
}
const merged = __assign({}, { a: 1 }, { b: 2 }, { a: 99 });
console.log(merged.a, merged.b);

// __rest (tsc destructuring rest helper)
function __rest(source: any, excluded: string[]) {
  const result: any = {};
  for (const key of Object.keys(source)) {
    if (!excluded.includes(key)) result[key] = source[key];
  }
  return result;
}
const { a: _a, ...rest } = { a: 1, b: 2, c: 3 };
console.log(rest.b, rest.c);

// __spreadArray (tsc array spread helper)
function __spreadArray(to: any[], from: any[]) {
  return to.concat(Array.prototype.slice.call(from));
}
const combined = __spreadArray([1, 2], [3, 4]);
console.log(combined.join(","));

// __extends (tsc class inheritance helper)
function __extends(d: any, b: any) {
  Object.setPrototypeOf(d, b);
  function Ctor() { this.constructor = d; }
  Ctor.prototype = b.prototype;
  d.prototype = new (Ctor as any)();
}
function Base2(this: any) { this.type = "base"; }
(Base2 as any).prototype.greet = function() { return "hello from " + this.type; };
function Derived(this: any) { Base2.call(this); this.type = "derived"; }
__extends(Derived, Base2);
const inst = new (Derived as any)();
console.log(inst.greet());
console.log(inst instanceof Derived);

// __awaiter pattern (tsc async helper — sync version for testing)
function __awaiter(thisArg: any, _args: any, _P: any, generator: any) {
  return new Promise((resolve) => {
    const g = generator.call(thisArg);
    function step(result: any) {
      if (result.done) { resolve(result.value); return; }
      Promise.resolve(result.value).then(v => step(g.next(v)));
    }
    step(g.next());
  });
}
__awaiter(null, [], null, function*() {
  const v = yield Promise.resolve(7);
  console.log("awaiter result: " + v);
});
