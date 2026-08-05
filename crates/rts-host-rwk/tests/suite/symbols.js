// `Symbol`, and the iteration protocol it made reachable.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

// Identity, which is why a symbol is a cell rather than a tag over its
// description: an interned encoding would have made these equal.
check("distinct", Symbol("a") !== Symbol("a"));
check("typeof", typeof Symbol("a") === "symbol");
check("typeof-well-known", typeof Symbol.iterator === "symbol");
check("description", Symbol("a").description === "a");
check("to-string", Symbol("a").toString() === "Symbol(a)");
check("to-string-anonymous", Symbol().toString() === "Symbol()");

// One value per name, or a property written under it could never be read back.
check("well-known-stable", Symbol.iterator === Symbol.iterator);
check("for-stable", Symbol.for("x") === Symbol.for("x"));
check("key-for", Symbol.keyFor(Symbol.for("x")) === "x");
check("key-for-unregistered", Symbol.keyFor(Symbol("x")) === undefined);
// Two key spaces, deliberately: colliding them is the bug the engine being
// replaced documents a warning about.
check("registry-is-not-well-known", Symbol.for("iterator") !== Symbol.iterator);

check("has-instance", typeof Symbol.hasInstance === "symbol");
check("to-primitive", typeof Symbol.toPrimitive === "symbol");
check("to-string-tag", typeof Symbol.toStringTag === "symbol");
check("async-iterator", typeof Symbol.asyncIterator === "symbol");

let k = Symbol("k");
let o = {};
o[k] = 7;
check("stored", o[k] === 7);
check("distinct-properties", (function () {
    let a = Symbol("k");
    let b = Symbol("k");
    let held = {};
    held[a] = 1;
    held[b] = 2;
    return held[a] === 1 && held[b] === 2;
})());

// Not enumerated, which is the whole cost of encoding the key as a reserved
// name rather than as a third kind of key.
check("not-in-keys", Object.keys(o).length === 0);
check("not-in-for-in", (function () {
    let n = 0;
    for (let key in o) { n = n + 1; }
    return n === 0;
})());
check("not-in-json", JSON.stringify(o) === "{}");
check("still-own", o.hasOwnProperty !== undefined);

// The protocol: an object that declares how it iterates is walked.
let counter = {};
counter[Symbol.iterator] = function () {
    let i = 0;
    return {
        next: function () {
            i = i + 1;
            if (i > 3) { return {done: true, value: undefined}; }
            return {done: false, value: i};
        }
    };
};
let total = 0;
for (let v of counter) { total = total + v; }
check("for-of-protocol", total === 6);
check("spread-protocol", [...counter].length === 3);
check("from-protocol", Array.from(counter).length === 3);

// An object declaring nothing walks zero times rather than failing — the
// stated gap while a throw cannot reach a handler in a caller.
check("non-iterable", (function () {
    let n = 0;
    for (let v of {a: 1}) { n = n + 1; }
    return n === 0;
})());

return failed;
