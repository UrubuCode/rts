// Objects, prototypes, accessors, and enumeration order.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

let o = {a: 1, b: 2};
check("read", o.a === 1);
check("write", (o.c = 3) === 3 && o.c === 3);
check("absent", o.zz === undefined);
check("computed", o["a"] === 1);
check("in", "a" in o);
check("not-in", ("zz" in o) === false);
check("delete", delete o.c && o.c === undefined);

check("keys", Object.keys({x: 1, y: 2}).length === 2);
check("values", Object.values({x: 1}).length === 1);
check("assign", Object.assign({}, {x: 1}).x === 1);

// Enumeration is index keys first in ascending numeric order, then the other
// strings in insertion order. Not insertion order overall, which is what a
// single slot list would have given.
let mixed = {};
mixed.b = 1;
mixed[2] = 1;
mixed.a = 1;
mixed[1] = 1;
check("order", Object.keys(mixed).join(",") === "1,2,b,a");

// Every plain object inherits from `Object.prototype`.
check("has-own", ({a: 1}).hasOwnProperty("a"));
check("has-own-absent", ({a: 1}).hasOwnProperty("b") === false);
check("object-to-string", ({}).toString() === "[object Object]");
check("value-of", (function () { let x = {}; return x.valueOf() === x; })());
check("instance-of-object", ({}) instanceof Object);

Object.prototype.shared = 7;
check("inherited", ({}).shared === 7);
check("inherited-not-own", ({}).hasOwnProperty("shared") === false);
check("inherited-not-enumerated", Object.keys({}).length === 0);

let child = {};
let parent = {p: 1};
Object.setPrototypeOf(child, parent);
check("set-prototype", child.p === 1);
check("get-prototype", Object.getPrototypeOf(child) === parent);
check("is-prototype-of", parent.isPrototypeOf(child));

// The walk carries the ORIGINAL receiver, so an inherited getter sees the
// object the read was written on.
let base = {get who() { return this.tag; }};
let derived = {tag: "derived"};
Object.setPrototypeOf(derived, base);
check("getter-receiver", derived.who === "derived");

// The walk stops on a descriptor, not on a value: an own property explicitly
// `undefined` shadows the parent.
let shadowing = {v: undefined};
Object.setPrototypeOf(shadowing, {v: 9});
check("shadow-with-undefined", shadowing.v === undefined);

let counted = 0;
let watched = {get n() { counted = counted + 1; return 5; }};
check("getter-runs", watched.n === 5 && counted === 1);
check("getter-runs-again", watched.n === 5 && counted === 2);

let stored = {};
Object.defineProperty(stored, "d", {value: 4});
check("define-value", stored.d === 4);

// A class is a constructor and an object of methods.
class Point {
    constructor(x, y) { this.x = x; this.y = y; }
    sum() { return this.x + this.y; }
    static origin() { return new Point(0, 0); }
}
check("class-field", new Point(1, 2).x === 1);
check("class-method", new Point(1, 2).sum() === 3);
check("class-static", Point.origin().sum() === 0);
check("class-instance-of", new Point(1, 2) instanceof Point);

class Shifted extends Point {
    constructor(x) { super(x, 10); }
    sum() { return super.sum() + 100; }
}
check("subclass-super-construct", new Shifted(1).y === 10);
check("subclass-super-method", new Shifted(1).sum() === 111);
check("subclass-instance-of-parent", new Shifted(1) instanceof Point);

// A constructor that returns an object produces that one.
function Factory() { return {made: true}; }
check("constructor-returns", new Factory().made === true);

check("typeof-object", typeof {} === "object");
check("typeof-null", typeof null === "object");
check("typeof-function", typeof function () {} === "function");
check("typeof-undefined", typeof undefined === "undefined");

// A computed CALL carries the object as its receiver, exactly as a named one
// does. It did not: `o["m"]()` fell into the plain-call path and ran with
// `undefined` as `this`.
check("computed-call-receiver", (function () {
    let o = {n: 7, read: function () { return this.n; }};
    return o["read"]() === 7;
})());
check("computed-call-through-a-variable", (function () {
    let o = {n: 7, read: function () { return this.n; }};
    let k = "read";
    return o[k]() === 7;
})());
check("computed-call-on-an-array", (function () {
    let a = [1];
    a["push"](2);
    return a.length === 2 && a[1] === 2;
})());
// The object is evaluated ONCE, which is why the receiver and the read share a
// value rather than each emitting the object.
check("computed-call-evaluates-once", (function () {
    let n = 0;
    function make() { n = n + 1; return {m: function () { return 1; }}; }
    make()["m"]();
    return n === 1;
})());

// A literal computed key takes the NAMED path — the inline cache — because the
// compiler already knows the name. Measured at 150x before this.
check("literal-key-reads", (function () {
    let o = {alpha: 1};
    return o["alpha"] === 1;
})());
check("literal-key-writes", (function () {
    let o = {};
    o["alpha"] = 2;
    return o.alpha === 2 && o["alpha"] === 2;
})());
check("literal-key-compound", (function () {
    let o = {alpha: 1};
    o["alpha"] = o["alpha"] + 4;
    return o.alpha === 5;
})());
check("literal-and-named-are-one-property", (function () {
    let o = {};
    o.alpha = 1;
    o["alpha"] = 2;
    return o.alpha === 2 && Object.keys(o).length === 1;
})());
// The trap: an all-digit key on an ARRAY reads the element, and a named read
// never asks about elements. Refused conservatively, so these still work.
check("digit-key-on-an-array", (function () {
    let a = [7, 8];
    return a["0"] === 7 && a["1"] === 8;
})());
check("digit-key-writes-an-element", (function () {
    let a = [7];
    a["0"] = 9;
    return a[0] === 9 && a.length === 1;
})());
check("digit-key-on-an-object", (function () {
    let o = {};
    o["0"] = 3;
    return o[0] === 3 && o["0"] === 3;
})());

let pairs = { a: 1, b: 2 };
check("entries-key", Object.entries(pairs)[1][0] === "b");
check("entries-value", Object.entries(pairs)[1][1] === 2);
check("entries-length", Object.entries(pairs).length === 2);
check("from-entries", Object.fromEntries([["x", 5]]).x === 5);
// `for-of` over a Map yields exactly the pairs `fromEntries` reads.
let source = new Map();
source.set("y", 6);
check("from-entries-map", Object.fromEntries(source).y === 6);

// `hasOwn` answers for a key holding `undefined`, which is why it cannot be
// written as a read compared against `undefined`.
let held = { present: undefined };
check("object-has-own", Object.hasOwn(held, "present"));
check("object-has-own-absent", Object.hasOwn(held, "missing") === false);

check("is-nan", Object.is(NaN, NaN));
check("is-zero", Object.is(0, -0) === false);
check("is-same", Object.is("a", "a"));

let ancestor = { inherited: 1 };
let made = Object.create(ancestor);
check("create-inherits", made.inherited === 1);
check("create-own-empty", Object.keys(made).length === 0);
check("create-null", Object.getPrototypeOf(Object.create(null)) === null);

let described = Object.create(ancestor, { own: { value: 3 } });
check("create-descriptors", described.own === 3);

let target = {};
Object.defineProperties(target, { one: { value: 1 }, two: { value: 2 } });
check("define-properties", target.one + target.two === 3);

check("own-property-names", Object.getOwnPropertyNames(pairs).length === 2);
check("descriptor-value", Object.getOwnPropertyDescriptor(pairs, "a").value === 1);
check("descriptor-writable", Object.getOwnPropertyDescriptor(pairs, "a").writable);
check("descriptor-absent", Object.getOwnPropertyDescriptor(pairs, "z") === undefined);
check("descriptors-all", Object.getOwnPropertyDescriptors(pairs).b.value === 2);

let accessor = {};
Object.defineProperty(accessor, "computed", { get: function () { return 4; } });
check("descriptor-getter", typeof Object.getOwnPropertyDescriptor(accessor, "computed").get === "function");
check("descriptor-getter-not-value", Object.getOwnPropertyDescriptor(accessor, "computed").value === undefined);

// A freeze has to survive a WARMED inline cache: the first loop teaches the
// store site the object's layout, and without the retype plus the store
// resolver the writes after the freeze would go straight through it.
// ONE store site, run on both sides of the freeze. Two loops would not test
// this: each `o.n = v` in the source is its own site with its own cold cache,
// so the second one would ask the runtime and be refused for the wrong reason.
check("freeze-beats-a-warm-cache", (function () {
    function write(target, v) { target.n = v; }
    let o = { n: 0 };
    write(o, 1);
    write(o, 2);
    Object.freeze(o);
    write(o, 99);
    write(o, 98);
    return o.n === 2;
})());
check("freeze-still-reads", (function () {
    let o = { a: 1, b: 2 };
    Object.freeze(o);
    return o.a === 1 && o.b === 2;
})());
check("freeze-refuses-new", (function () {
    let o = {};
    Object.freeze(o);
    o.fresh = 1;
    return o.fresh === undefined;
})());
check("freeze-refuses-delete", (function () {
    let o = { a: 1 };
    Object.freeze(o);
    return (delete o.a) === false && o.a === 1;
})());
check("is-frozen", (function () {
    let o = { a: 1 };
    return Object.isFrozen(o) === false && Object.isFrozen(Object.freeze(o));
})());
check("frozen-descriptor", (function () {
    let o = { a: 1 };
    Object.freeze(o);
    let d = Object.getOwnPropertyDescriptor(o, "a");
    return d.writable === false && d.configurable === false;
})());

// Sealed is the middle: writes still land, the shape may not change.
check("seal-allows-writes", (function () {
    let o = { n: 1 };
    Object.seal(o);
    o.n = 5;
    return o.n === 5;
})());
check("seal-refuses-new", (function () {
    let o = { n: 1 };
    Object.seal(o);
    o.other = 2;
    return o.other === undefined;
})());
check("seal-refuses-delete", (function () {
    let o = { n: 1 };
    Object.seal(o);
    return (delete o.n) === false;
})());
check("is-sealed", (function () {
    let o = { n: 1 };
    return Object.isSealed(o) === false && Object.isSealed(Object.seal(o));
})());

// `preventExtensions` refuses only growth.
check("prevent-extensions-writes", (function () {
    let o = { n: 1 };
    Object.preventExtensions(o);
    o.n = 3;
    return o.n === 3;
})());
check("prevent-extensions-refuses-new", (function () {
    let o = { n: 1 };
    Object.preventExtensions(o);
    o.other = 1;
    return o.other === undefined;
})());
check("prevent-extensions-allows-delete", (function () {
    let o = { n: 1 };
    Object.preventExtensions(o);
    return (delete o.n) === true && o.n === undefined;
})());
check("is-extensible", (function () {
    let o = {};
    return Object.isExtensible(o) && Object.isExtensible(Object.preventExtensions(o)) === false;
})());
// One-way: a weaker level must not thaw a stronger one.
check("prevent-extensions-does-not-thaw", (function () {
    let o = { n: 1 };
    Object.freeze(o);
    Object.preventExtensions(o);
    o.n = 7;
    return o.n === 1;
})());

// A descriptor's flags are per OBJECT per property: `defineProperty` on one
// object says nothing about another sharing its shape.
check("writable-false-refuses", (function () {
    let o = {};
    Object.defineProperty(o, "fixed", { value: 1, writable: false });
    o.fixed = 9;
    return o.fixed === 1;
})());
// The same warm-cache case a freeze has to survive, for one property.
check("writable-false-beats-a-warm-cache", (function () {
    function write(target, v) { target.n = v; }
    let o = { n: 0 };
    write(o, 1);
    write(o, 2);
    Object.defineProperty(o, "n", { value: 2, writable: false });
    write(o, 99);
    return o.n === 2;
})());
check("writable-false-is-per-object", (function () {
    let a = { shared: 1 };
    let b = { shared: 1 };
    Object.defineProperty(a, "shared", { value: 1, writable: false });
    b.shared = 5;
    return b.shared === 5 && a.shared === 1;
})());
check("enumerable-false-hides", (function () {
    let o = { seen: 1 };
    Object.defineProperty(o, "hidden", { value: 2, enumerable: false });
    return Object.keys(o).join(",") === "seen" && o.hidden === 2;
})());
check("enumerable-false-not-in-for-in", (function () {
    let o = {};
    Object.defineProperty(o, "hidden", { value: 2, enumerable: false });
    let count = 0;
    for (let k in o) { count = count + 1; }
    return count === 0;
})());
// `getOwnPropertyNames` reports what an enumeration does not, which is the
// whole difference between it and `Object.keys`.
check("own-property-names-includes-hidden", (function () {
    let o = {};
    Object.defineProperty(o, "hidden", { value: 2, enumerable: false });
    return Object.getOwnPropertyNames(o).length === 1 && Object.keys(o).length === 0;
})());
check("configurable-false-refuses-delete", (function () {
    let o = {};
    Object.defineProperty(o, "fixed", { value: 1, configurable: false });
    return (delete o.fixed) === false && o.fixed === 1;
})());
// A field left out of a descriptor is FALSE, where an ordinary assignment gives
// all three. That is what programs reach for `defineProperty` for.
check("descriptor-defaults-to-false", (function () {
    let o = {};
    Object.defineProperty(o, "x", { value: 1 });
    let d = Object.getOwnPropertyDescriptor(o, "x");
    return d.writable === false && d.enumerable === false && d.configurable === false;
})());
check("assignment-defaults-to-true", (function () {
    let o = { x: 1 };
    let d = Object.getOwnPropertyDescriptor(o, "x");
    return d.writable && d.enumerable && d.configurable;
})());

// The two the engine already treated specially are now ordinary non-enumerable
// properties rather than names the enumeration knew to skip.
check("array-length-not-enumerated", Object.keys([1, 2]).join(",") === "0,1");
check("map-size-not-enumerated", (function () {
    let m = new Map();
    m.set("k", 1);
    return Object.keys(m).length === 0 && m.size === 1;
})());

return failed;
