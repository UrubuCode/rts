// Objects the compiler replaces with locals, and objects it must not.
//
// `emit/escape.rs` removes an object literal bound to a local that provably
// never leaves the function, turning each property into an ordinary binding.
// That is invisible when it is right and a miscompile when it is wrong, so
// every check here has a twin: the same program written so the object escapes.
// If a pair ever disagrees, the analysis and the runtime have diverged.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

// Something opaque to hand an object to. The compiler cannot see that `keep`
// does not store its argument, which is exactly the point: a call is an escape.
function keep(x) { return x; }

// -- the replaced shape ------------------------------------------------------

let a = {x: 1, y: 2};
check("read", a.x === 1 && a.y === 2);

let b = {x: 1};
b.x = 5;
check("write", b.x === 5);

let c = {n: 1};
c.n += 4;
c.n *= 2;
check("compound", c.n === 10);

// Shorthand records `v: v`, so it is decided by the same rule as any identifier
// value.
let v = 7;
let d = {v};
check("shorthand", d.v === 7);

// Two paths writing the same property have to merge, which is why the property
// is a binding rather than a value: the merge is the scope's, not ours.
let e = {k: 0};
if (v > 3) { e.k = 1; } else { e.k = 2; }
check("merge", e.k === 1);

// A fresh object per iteration, replaced or not, is a fresh set of values.
let total = 0;
for (let i = 0; i < 4; i = i + 1) {
    let f = {p: i, q: i + 1};
    total = total + f.p + f.q;
}
check("loop", total === 16);

// An inner declaration shadows, and reading the outer one after the block must
// still find the outer one.
let g = {x: 1};
{
    let g2 = {x: 9};
    check("shadow-inner", g2.x === 9);
}
check("shadow-outer", g.x === 1);

// -- property values that do something ---------------------------------------

// The values run in source order, once each, at the point the literal is
// written. That is the whole ordering obligation, and it is what allows a value
// to be any expression rather than only a literal or a name.
let order = "";
function note(tag, value) { order = order + tag; return value; }
let ord = {a: note("a", 1), b: note("b", 2), c: note("c", 3)};
check("order", order === "abc");
check("order-values", ord.a + ord.b + ord.c === 6);

// Once each, not once per read.
let calls = 0;
function counted() { calls = calls + 1; return 9; }
let once = {a: counted()};
check("evaluated-once", once.a + once.a === 18 && calls === 1);

// A computed value, which is the shape the old rule refused outright.
let base = 10;
let computed = {a: base, b: base + 1, c: base * 2};
check("computed-values", computed.a + computed.b + computed.c === 41);

// Reading the object inside its own initialiser — `let o = {a: 1, b: o.a}` — is
// a temporal dead zone reference and should throw. There is no check for it
// here, and that is deliberate rather than an omission: this engine does not
// implement the dead zone yet, so the program does not throw whether or not the
// object is replaced, and a check would pass by agreeing with a bug.
//
// The analysis refuses it regardless, which is the half that is this file's to
// keep: `Escaping::declaring`, pinned by two unit tests in `escape.rs`. When the
// dead zone lands, replacement will already be out of its way.

// -- the same programs, escaping ---------------------------------------------

let h = keep({x: 1, y: 2});
check("escaped-read", h.x === 1 && h.y === 2);

let i2 = {x: 1};
keep(i2);
i2.x = 5;
check("escaped-write", i2.x === 5);

// Returned, and therefore not replaceable.
function makes() { let o = {x: 3}; return o; }
check("returned", makes().x === 3);

// A method call makes the object the receiver, which is a use the analysis
// refuses — `this` has to be an object.
let j = {x: 4, get: function () { return this.x; }};
check("receiver", j.get() === 4);

// A computed key cannot be matched against the literal's names.
let k = {x: 1, y: 2};
let which = "y";
check("computed", k[which] === 2);

// Identity survives: two replaced-looking literals are still two objects, and a
// name that reaches an object twice reaches the same one.
let l = {x: 1};
let m = l;
m.x = 2;
check("aliased", l.x === 2);
check("distinct", ({x: 1}) !== ({x: 1}));

// `delete` and `in` need a real object to answer about.
let n = {x: 1};
check("in", "x" in n);
check("delete", delete n.x && n.x === undefined);

// Enumeration reads keys no access named, so the object has to be real.
let p = {x: 1, y: 2};
check("keys", Object.keys(p).join(",") === "x,y");

return failed;
