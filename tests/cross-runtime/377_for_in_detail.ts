// Cross-runtime: for...in edge cases — enumerability, prototype chain, order.

// only enumerable own + inherited enumerable
const parent: any = { inherited: true };
const child: any = Object.create(parent);
child.own1 = 1;
child.own2 = 2;
Object.defineProperty(child, "hidden", { value: 99, enumerable: false });

const keys: string[] = [];
for (const k in child) keys.push(k);
console.log(keys.join(","));  // own1,own2,inherited (hidden excluded)

// hasOwnProperty filter (classic pattern)
const ownOnly: string[] = [];
for (const k in child) if (Object.prototype.hasOwnProperty.call(child, k)) ownOnly.push(k);
console.log(ownOnly.join(","));  // own1,own2

// for...in on array (generally avoided — shows indices as strings)
const arr = ["a", "b", "c"];
const arrKeys: string[] = [];
for (const k in arr) arrKeys.push(typeof k + ":" + k);
console.log(arrKeys.join(","));  // string:0,string:1,string:2

// for...in skips non-enumerable prototype methods
class Foo { method() {} }
const f = new Foo();
(f as any).x = 1;
const fKeys: string[] = [];
for (const k in f) fKeys.push(k);
console.log(fKeys.join(","));  // x (method not enumerable via class syntax)

// for...in vs for...of on Map
const m = new Map([["a", 1], ["b", 2]]);
const inKeys: string[] = [];
for (const k in m) inKeys.push(k);  // nothing — Map has no enumerable own props
console.log(inKeys.length);  // 0

const ofPairs: string[] = [];
for (const [k, v] of m) ofPairs.push(k + "=" + v);
console.log(ofPairs.join(","));  // a=1,b=2

// deletion during for...in (implementation-defined but consistent per runtime)
const obj: any = { a: 1, b: 2, c: 3 };
const visited: string[] = [];
for (const k in obj) {
  visited.push(k);
  if (k === "a") delete obj.b;  // b may or may not appear
}
console.log(visited.includes("a"));   // true
console.log(visited.includes("c"));   // true
