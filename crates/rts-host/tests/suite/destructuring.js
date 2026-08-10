// Destructuring: binding patterns in a declaration and a `for-of` head.
// The assignment form ([a, b] = xs) is not covered here -- it is still
// refused by the emitter (see crates/rts-codegen/src/emit/destructure.rs).
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

// Array pattern, plain.
let [a, b] = [1, 2];
check("array-basic", a === 1 && b === 2);

// Array pattern, a hole.
let [, second] = [1, 2];
check("array-hole", second === 2);

// Array pattern over an iterable that is not an array -- the iteration
// protocol, not indexing.
let seen = "";
for (let [k, v] of [["x", 1], ["y", 2]]) {
  seen = seen + k + v;
}
check("array-in-for-of", seen === "x1y2");

// Object pattern, plain.
let { p, q } = { p: 1, q: 2 };
check("object-basic", p === 1 && q === 2);

// Renaming: `{a: b}` binds `b`, reading the property named `a`.
let { a: renamed } = { a: 42 };
check("object-rename", renamed === 42);

// Nested, both directions, and mixed.
let { outer: { inner } } = { outer: { inner: "deep" } };
check("object-nested", inner === "deep");
let [[first]] = [[7]];
check("array-nested", first === 7);
let { list: [x0, x1] } = { list: [10, 20] };
check("mixed-nested", x0 === 10 && x1 === 20);

// A nested array pattern where the outer pattern has more than one slot --
// this is the shape that catches a synthetic temporary leaking into an
// ancestor pattern's still-live one of the same name.
let [[oa0], [oa1]] = [[1], [2]];
check("array-nested-siblings", oa0 === 1 && oa1 === 2);

// Defaults apply only to `undefined`, never to `null` and never by falsiness.
let { d1 = 9 } = {};
check("default-missing", d1 === 9);
let { d2 = 9 } = { d2: undefined };
check("default-explicit-undefined", d2 === 9);
let { d3 = 9 } = { d3: null };
check("default-not-for-null", d3 === null);
let { d4 = 9 } = { d4: 0 };
check("default-not-for-falsy", d4 === 0);

// A default is evaluated only when it is needed.
let calls = 0;
function bump() { calls = calls + 1; return 5; }
let { d5 = bump() } = { d5: 1 };
check("default-not-called-when-present", calls === 0 && d5 === 1);
let { d6 = bump() } = {};
check("default-called-when-missing", calls === 1 && d6 === 5);

// Rest, in both shapes.
let [head, ...tail] = [1, 2, 3];
check("array-rest", head === 1 && tail.length === 2 && tail[0] === 2 && tail[1] === 3);
let [onlyHead, ...emptyTail] = [1];
check("array-rest-empty", onlyHead === 1 && emptyTail.length === 0);

let { r1, ...rest } = { r1: 1, r2: 2, r3: 3 };
check("object-rest", r1 === 1 && rest.r2 === 2 && rest.r3 === 3 && !("r1" in rest));

// A computed key's expression runs exactly once, in source order -- and
// `...rest` excludes it by the value it produced, not by its spelling.
let log = "";
function trace(x) { log = log + x; return x; }
let { [trace("k1")]: v1, [trace("k2")]: v2, ...restc } = { k1: "one", k2: "two", k3: "three" };
check("computed-key-order", log === "k1k2" && v1 === "one" && v2 === "two");
check("computed-key-excluded-from-rest", restc.k3 === "three" && !("k1" in restc) && !("k2" in restc));

// The source is evaluated exactly once, however many names the pattern reads.
let sourceCalls = 0;
function sourceOnce() { sourceCalls = sourceCalls + 1; return [1, 2]; }
let [oa, ob] = sourceOnce();
check("source-evaluated-once", sourceCalls === 1 && oa === 1 && ob === 2);

return failed;

// The ASSIGNMENT role: no names introduced, arbitrary targets written. A bare
// name here writes a binding that already exists.
let pp = 0;
let qq = 0;
[pp, qq] = [7, 8];
check("assign-array", pp === 7 && qq === 8);
({ pp, qq } = { pp: 1, qq: 2 });
check("assign-object", pp === 1 && qq === 2);
// A property is a legal target, which is the whole reason the assignment role
// exists apart from the binding one.
let box = {};
[box.first] = [5];
check("assign-into-a-property", box.first === 5);
// The source is evaluated once, and the expression produces it.
let sourced = 0;
function made() { sourced = sourced + 1; return [1, 2]; }
let produced = ([pp, qq] = made());
check("assign-evaluates-source-once", sourced === 1);
check("assign-produces-the-source", produced[0] === 1 && produced.length === 2);
