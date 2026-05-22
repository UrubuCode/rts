// Cross-runtime: Proxy advanced traps — construct, apply, ownKeys, getOwnPropertyDescriptor.

// apply trap (intercept function calls)
function greet(name: string) { return "Hello, " + name; }
const pGreet = new Proxy(greet, {
  apply(target, thisArg, args) {
    return target.apply(thisArg, args).toUpperCase();
  }
});
console.log(pGreet("world"));   // HELLO, WORLD

// construct trap
class Point { constructor(public x: number, public y: number) {} }
const PPoint = new Proxy(Point, {
  construct(Target, args) {
    console.log("new Point(" + args.join(",") + ")");
    return new Target(...args as [number, number]);
  }
});
const p = new PPoint(3, 4);
console.log(p.x + "," + p.y);  // 3,4
console.log(p instanceof Point); // true

// ownKeys trap (controls Object.keys, for...in, etc.)
const secret = { public1: 1, public2: 2, _private: 3, __internal: 4 };
const filtered = new Proxy(secret, {
  ownKeys(target) {
    return Object.keys(target).filter(k => !k.startsWith("_"));
  },
  getOwnPropertyDescriptor(target, key) {
    if (String(key).startsWith("_")) return undefined;
    return Object.getOwnPropertyDescriptor(target, key);
  }
});
console.log(Object.keys(filtered).join(","));  // public1,public2

// getOwnPropertyDescriptor trap
const virtualized = new Proxy({} as any, {
  getOwnPropertyDescriptor(_, key) {
    return { value: String(key).toUpperCase(), writable: true, enumerable: true, configurable: true };
  },
  ownKeys() { return ["foo", "bar"]; }
});
console.log(Object.keys(virtualized).join(","));  // foo,bar
console.log(virtualized.foo);   // FOO (via get — no get trap so falls through)

// has trap with ownKeys
const range = new Proxy({} as any, {
  has(_, key) { const n = Number(key); return Number.isInteger(n) && n >= 0 && n < 10; },
  get(_, key) { const n = Number(key); return n * n; }
});
console.log(5 in range);    // true
console.log(15 in range);   // false
console.log(range[7]);      // 49

// revocable proxy
const { proxy: rev, revoke } = Proxy.revocable({ x: 1 }, {});
console.log(rev.x);  // 1
revoke();
try { console.log(rev.x); } catch (e: any) { console.log("revoked:" + e.constructor.name); }
