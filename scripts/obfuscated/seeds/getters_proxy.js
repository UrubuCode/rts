// Accessors, defineProperty, Proxy traps, Reflect, symbols as keys.
const out = [];
const o = { _v: 1, get v() { return this._v * 10; }, set v(n) { this._v = n; } };
o.v = 5;
out.push(o.v, o._v);
out.push(Object.keys(o).join(","));
const d = Object.getOwnPropertyDescriptor(o, "v");
out.push(typeof d.get, typeof d.set, d.enumerable);
const target = { a: 1 };
const log = [];
const p = new Proxy(target, {
  get(t, k, r) { if (typeof k === "string") log.push("g:" + k); return Reflect.get(t, k, r); },
  set(t, k, v, r) { log.push("s:" + String(k)); return Reflect.set(t, k, v, r); },
  has(t, k) { log.push("h:" + String(k)); return Reflect.has(t, k); },
});
p.a; p.b = 2; "a" in p;
out.push(log.join(","), target.b);
out.push(Reflect.ownKeys({ z: 1, 2: 2, a: 3 }).join(","));
const chained = Object.create({ inherited: "up" });
chained.own = "here";
out.push(chained.inherited, Object.keys(chained).join(","));
out.push(Reflect.getPrototypeOf(chained).inherited);
const counter = { n: 0 };
Object.defineProperty(counter, "next", { get() { return ++this.n; } });
out.push(counter.next, counter.next, counter.n);
console.log(out.join("|"));
