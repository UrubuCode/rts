// ONE thing: an array iterator holds an INDEX into the live array, not a
// snapshot — growing, shrinking and reassigning behind it are all observable.
const a = [1, 2, 3];
const it = a[Symbol.iterator]();
console.log("first=" + JSON.stringify(it.next()));
a.push(4);
console.log("afterPush=" + JSON.stringify(it.next()));
a[2] = 99;
console.log("mutated=" + JSON.stringify(it.next()));
console.log("last=" + JSON.stringify(it.next()));
console.log("done=" + JSON.stringify(it.next()));
console.log("pastDone=" + JSON.stringify(it.next()));

// Truncating the array ends the iterator early.
const b = [1, 2, 3, 4];
const ib = b[Symbol.iterator]();
ib.next();
b.length = 1;
console.log("truncated=" + JSON.stringify(ib.next()));

// Two iterators of the same array are independent.
const c = [1, 2];
const i1 = c[Symbol.iterator]();
const i2 = c[Symbol.iterator]();
console.log("i1a=" + i1.next().value + " i2a=" + i2.next().value + " i1b=" + i1.next().value);
console.log("distinct=" + (i1 !== i2));

// keys/values/entries are separate iterators over the same index.
const d = ["x", "y"];
const k = d.keys(), v = d.values(), e = d.entries();
console.log("k=" + k.next().value + " v=" + v.next().value + " e=" + JSON.stringify(e.next().value));

// The result object is fresh each call, so holding one is safe.
const f = [1, 2];
const fi = f[Symbol.iterator]();
const r1 = fi.next();
const r2 = fi.next();
console.log("freshResult=" + (r1 !== r2) + " r1=" + r1.value + " r2=" + r2.value);

// An array iterator is itself iterable and returns itself.
const g = [1][Symbol.iterator]();
console.log("selfIterable=" + (g[Symbol.iterator]() === g));

// The iterator prototype is shared across keys/values/entries.
console.log("sharedProto=" + (Object.getPrototypeOf([].keys()) === Object.getPrototypeOf([].values())));
console.log("tag=" + Object.prototype.toString.call([].values()));

// A hole yields undefined through the iterator, unlike forEach which skips it.
const h: any[] = [1, , 3];
console.log("iterHoles=" + Array.from(h).map(String).join(","));
let visited = 0;
h.forEach(() => visited++);
console.log("forEachVisits=" + visited);
