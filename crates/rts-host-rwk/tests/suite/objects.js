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

return failed;
