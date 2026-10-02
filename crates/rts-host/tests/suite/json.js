// `JSON.stringify` and `JSON.parse`.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

check("object", JSON.stringify({a: 1, b: "x"}) === "{\"a\":1,\"b\":\"x\"}");
check("array", JSON.stringify([1, 2, 3]) === "[1,2,3]");
check("nested", JSON.stringify({a: [1, {b: 2}]}) === "{\"a\":[1,{\"b\":2}]}");
check("string", JSON.stringify("a") === "\"a\"");
check("number", JSON.stringify(1.5) === "1.5");
check("true", JSON.stringify(true) === "true");
check("null", JSON.stringify(null) === "null");
check("empty-object", JSON.stringify({}) === "{}");
check("empty-array", JSON.stringify([]) === "[]");

// Non-finite is `null`, which is the specification's answer rather than an
// approximation of one.
check("nan", JSON.stringify([0 / 0]) === "[null]");
check("infinity", JSON.stringify([1 / 0]) === "[null]");

// `undefined` is dropped as a member and becomes `null` as an element — two
// answers for one value, which is the pair an implementation gets wrong by
// treating them alike.
check("undefined-member", JSON.stringify({a: undefined, b: 1}) === "{\"b\":1}");
check("undefined-element", JSON.stringify([undefined]) === "[null]");
check("undefined-top", JSON.stringify(undefined) === undefined);
check("function-member", JSON.stringify({f: function () {}, b: 1}) === "{\"b\":1}");

// Escapes. A control character below 0x20 has to be escaped or the output is
// not JSON at all.
check("escape-quote", JSON.stringify("\"") === "\"\\\"\"");
check("escape-backslash", JSON.stringify("\\") === "\"\\\\\"");
check("escape-newline", JSON.stringify(JSON.parse("\"\\n\"")).length === 4);
check("escape-control", JSON.stringify(JSON.parse("\"\\u0001\"")).length === 8);

check("space-number", JSON.stringify({a: 1}, null, 2).length > 8);
check("space-string", JSON.stringify({a: 1}, null, "\t").length > 8);

check("parse-object", JSON.parse("{\"a\":[1,2]}").a[1] === 2);
check("parse-unicode", JSON.parse("\"\\u0041\"") === "A");
check("parse-negative-exponent", JSON.parse("-1.5e2") === -150);
check("parse-true", JSON.parse("true") === true);
check("parse-null", JSON.parse("null") === null);
check("parse-whitespace", JSON.parse("  [ 1 , 2 ]  ").length === 2);
check("parse-nested", JSON.parse("[[1]]")[0][0] === 1);

// A key that spells an index reaches the property either spelling finds, which
// is what routing the parse through the interner buys.
check("parse-index-key", JSON.parse("{\"0\":7}")[0] === 7);

// A parse error THROWS, which is what the specification says and what this
// used to deny: "answers `undefined`, where the specification throws — the same
// stated gap every operation has while a throw cannot find a handler". The gap
// closed and the three checks below went on passing for the wrong reason until
// the raise arrived, at which point the uncaught `SyntaxError` killed the whole
// run. Each one now asserts the refusal rather than the absence.
function parseThrows(text) {
    try { JSON.parse(text); return false; } catch (e) { return true; }
}
check("parse-truncated-throws", parseThrows("["));
check("parse-trailing-throws", parseThrows("1 2"));
check("parse-bare-word-throws", parseThrows("nope"));

check("round-trip", (function () {
    let o = {a: [1, {b: true}], c: null, d: "x"};
    return JSON.stringify(JSON.parse(JSON.stringify(o))) === JSON.stringify(o);
})());

// A cycle THROWS, which is what the specification says. This asserted that it
// "answers `null` rather than hanging" — a third behaviour that is neither
// Node's nor Bun's — and the day the raise arrived the uncaught `TypeError` took
// the run with it. Asserting the refusal is strictly more than asserting a
// placeholder, and the message is checked too, because "it threw" alone would
// also pass for a cycle that ran out of stack.
check("cycle-throws", (function () {
    let o = {};
    o.self = o;
    try {
        JSON.stringify(o);
        return false;
    } catch (e) {
        return String(e.message).indexOf("circular") >= 0;
    }
})());

// A getter runs, because members are read through the ordinary property path.
check("getter-member", JSON.stringify({get a() { return 3; }}) === "{\"a\":3}");

return failed;
