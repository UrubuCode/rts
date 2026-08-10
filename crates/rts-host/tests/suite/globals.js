// The global object, `Reflect`, URI encoding, and `structuredClone`.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("global-this", typeof globalThis === "object");
// A property put on the global object is readable from it. Reading it as a
// BARE name is refused instead, and deliberately: the compiler decides which
// names resolve from what the program assigns syntactically, so a name only the
// runtime ever created is an unbound name rather than an `undefined` that hides
// every typo.
check("global-this-holds", (function () {
    globalThis.mine = 5;
    return globalThis.mine === 5;
})());
// Assigning to an undeclared name DOES create one, which is what sloppy mode
// does and the only way a program without modules introduces a global.
created = 6;
check("assignment-creates", created === 6 && globalThis.created === 6);
check("one-object-per-name", Math === Math);
check("function-identity", parseInt === parseInt);
check("reachable-through-global", (function () {
    let p = parseInt;
    return globalThis.parseInt === p;
})());

check("reflect-get", Reflect.get({a: 1}, "a") === 1);
check("reflect-set", (function () {
    let o = {};
    Reflect.set(o, "a", 2);
    return o.a === 2;
})());
check("reflect-has", Reflect.has({a: 1}, "a") && !Reflect.has({}, "a"));
check("reflect-delete", (function () {
    let o = {a: 1};
    Reflect.deleteProperty(o, "a");
    return o.a === undefined;
})());
check("reflect-own-keys", Reflect.ownKeys({a: 1, b: 2}).length === 2);
check("reflect-apply", Reflect.apply(function (a, b) { return a + b; }, null, [1, 2]) === 3);
check("reflect-construct", (function () {
    class P { constructor(n) { this.n = n; } }
    return Reflect.construct(P, [5]).n === 5;
})());
check("reflect-get-prototype", (function () {
    let p = {};
    let c = {};
    Object.setPrototypeOf(c, p);
    return Reflect.getPrototypeOf(c) === p;
})());
// `Reflect.get` is the same read the syntax performs, so a getter runs.
check("reflect-runs-getter", Reflect.get({get a() { return 3; }}, "a") === 3);

check("encode-component", encodeURIComponent("a b") === "a%20b");
check("encode-component-slash", encodeURIComponent("a/b") === "a%2Fb");
check("encode-component-unreserved", encodeURIComponent("a-_.!~*'()") === "a-_.!~*'()");
check("encode-uri-keeps-reserved", encodeURI("a/b?c=d") === "a/b?c=d");
check("encode-uri-escapes-space", encodeURI("a b") === "a%20b");
check("encode-utf8", encodeURIComponent("é") === "%C3%A9");

check("decode-component", decodeURIComponent("a%20b") === "a b");
check("decode-component-slash", decodeURIComponent("a%2Fb") === "a/b");
// The asymmetry that is the whole difference between the two: `decodeURI`
// preserves an escaped reserved character.
check("decode-uri-preserves", decodeURI("a%2Fb") === "a%2Fb");
check("decode-uri-space", decodeURI("a%20b") === "a b");
check("decode-utf8", decodeURIComponent("%C3%A9") === "é");
// A malformed escape is a `URIError` in the specification; this answers
// `undefined`, because a throw here would end the program over one bad query
// parameter.
check("decode-malformed", decodeURIComponent("%zz") === undefined);

check("clone-primitive", structuredClone(5) === 5);
check("clone-string", structuredClone("a") === "a");
check("clone-object", (function () {
    let o = {a: 1, b: {c: 2}};
    let copy = structuredClone(o);
    return copy.a === 1 && copy.b.c === 2 && copy !== o && copy.b !== o.b;
})());
check("clone-array", (function () {
    let a = [1, [2]];
    let copy = structuredClone(a);
    return copy[1][0] === 2 && copy !== a;
})());
check("clone-date", structuredClone(new Date(1000)).getTime() === 1000);
check("clone-map", (function () {
    let m = new Map([[1, "a"]]);
    let copy = structuredClone(m);
    return copy.get(1) === "a" && copy !== m;
})());
check("clone-set", structuredClone(new Set([1, 2])).size === 2);
// A cycle terminates, which is what the memo buys and what a depth cap would
// have truncated silently instead.
check("clone-cycle", (function () {
    let o = {};
    o.self = o;
    let copy = structuredClone(o);
    return copy.self === copy && copy !== o;
})());
// A function is not cloneable. The specification throws; this leaves the
// position `undefined` and copies the rest.
check("clone-function", structuredClone({f: function () {}, a: 1}).a === 1);

return failed;
