"use strict";
// Pins two things: (1) in strict code a plain `o.k = v` throws a TypeError when
// k is an own read-only data property, a getter-only accessor, or an INHERITED
// read-only data property (frozen / non-writable prototype), while writes to
// ordinary writable properties still work; (2) Object(primitive) returns a
// wrapper object (not the bare primitive), Object(obj) is identity, and
// Object(null|undefined) is a fresh plain object.

function attempt(label: string, fn: () => unknown): void {
  try {
    console.log(label + "=ok:" + String(fn()));
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const base = Object.freeze({ ro: 1 });
const child: any = Object.create(base);
attempt("inherited_frozen", () => { child.ro = 2; return child.ro; });
console.log("child_own=" + Object.prototype.hasOwnProperty.call(child, "ro"));
attempt("inherited_new_key", () => { child.fresh = 5; return child.fresh; });

const proto2: any = {};
Object.defineProperty(proto2, "nw", { value: 10, writable: false });
const child2: any = Object.create(proto2);
attempt("inherited_nonwritable", () => { child2.nw = 3; return child2.nw; });
const grand: any = Object.create(child2);
attempt("inherited_two_deep", () => { grand.nw = 4; return grand.nw; });
// an own property shadows the read-only one, via defineProperty
Object.defineProperty(child2, "nw", { value: 7, writable: true, configurable: true });
attempt("shadow_then_write", () => { child2.nw = 8; return child2.nw; });

const own: any = {};
Object.defineProperty(own, "x", { value: 1, writable: false });
attempt("own_readonly", () => { own.x = 2; return own.x; });

const getterOnly: any = {};
Object.defineProperty(getterOnly, "g", { get() { return 1; } });
attempt("getter_only", () => { getterOnly.g = 2; return getterOnly.g; });

const inheritedGetter: any = Object.create(getterOnly);
attempt("inherited_getter_only", () => { inheritedGetter.g = 2; return inheritedGetter.g; });

// ordinary writes keep working
const plain: any = { a: 1 };
plain.a = 2;
plain.b = 3;
const sub: any = Object.create({ inheritedWritable: 1 });
sub.inheritedWritable = 9;
console.log("plain=" + plain.a + "," + plain.b + " sub=" + sub.inheritedWritable);
class P { v = 0; }
const ps: P[] = [new P(), new P()];
let sum = 0;
for (let i = 0; i < 1000; i++) { const o = ps[i & 1]; o.v = i; sum += o.v; }
console.log("hot=" + sum);

// Object(primitive)
const n: any = Object(7);
console.log("num=" + typeof n + "," + (n instanceof Number) + "," + n.valueOf());
const s: any = Object("s");
console.log("str=" + typeof s + "," + (s instanceof String) + "," + s.valueOf() + "," + s.length);
const b: any = Object(true);
console.log("bool=" + typeof b + "," + (b instanceof Boolean) + "," + b.valueOf());
const bi: any = Object(10n);
console.log("big=" + typeof bi + "," + (bi instanceof BigInt) + "," + (bi.valueOf() === 10n));
const sym = Symbol("q");
const sy: any = Object(sym);
console.log("sym=" + typeof sy + "," + (sy instanceof Symbol) + "," + (sy.valueOf() === sym));
const o1 = { z: 1 };
console.log("same=" + (Object(o1) === o1));
console.log("null=" + typeof Object(null) + "," + Object.keys(Object(null)).length);
console.log("undef=" + typeof Object(undefined) + "," + Object.keys(Object(undefined)).length);
console.log("assign_box=" + typeof Object.assign(7 as any, { a: 1 }));
