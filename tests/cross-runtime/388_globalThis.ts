// Cross-runtime: globalThis and cross-context globals.

// globalThis exists in all modern runtimes
console.log(typeof globalThis);          // object
console.log(globalThis === globalThis);  // true

// well-known globals accessible via globalThis
console.log(typeof globalThis.Math);         // object
console.log(typeof globalThis.JSON);         // object
console.log(typeof globalThis.Promise);      // function
console.log(typeof globalThis.Map);          // function
console.log(typeof globalThis.Set);          // function
console.log(typeof globalThis.WeakMap);      // function
console.log(typeof globalThis.Proxy);        // function
console.log(typeof globalThis.Reflect);      // object
console.log(typeof globalThis.Symbol);       // function
console.log(typeof globalThis.BigInt);       // function
console.log(typeof globalThis.parseInt);     // function
console.log(typeof globalThis.parseFloat);   // function
console.log(typeof globalThis.isNaN);        // function
console.log(typeof globalThis.isFinite);     // function
console.log(typeof globalThis.encodeURIComponent); // function

// globalThis for polyfill detection pattern (bundler/library pattern)
function ensureGlobal(key: string, factory: () => any) {
  if (!(key in globalThis)) {
    (globalThis as any)[key] = factory();
  }
  return (globalThis as any)[key];
}
const myLib = ensureGlobal("__myLib", () => ({ version: "1.0", ready: true }));
console.log(myLib.version);   // 1.0
console.log(myLib.ready);     // true
// Second call returns same instance
const myLib2 = ensureGlobal("__myLib", () => ({ version: "2.0", ready: false }));
console.log(myLib2.version);  // 1.0 (not replaced)

// globalThis identity checks
console.log(globalThis.Array === Array);    // true
console.log(globalThis.Object === Object);  // true
console.log(globalThis.Error === Error);    // true
