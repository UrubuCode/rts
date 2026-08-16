// Cross-runtime: which error CONSTRUCTOR each JSON failure produces —
// SyntaxError from the parser, TypeError from a cycle or a BigInt, and whatever
// a user callback threw passing through untouched. No built-in message is
// printed; only the constructor and the error's own identity.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

// The parser: every malformed input is a SyntaxError.
console.log("parse-empty=" + probe(() => JSON.parse("")));
console.log("parse-trailing-comma=" + probe(() => JSON.parse("[1,]")));
console.log("parse-single-quote=" + probe(() => JSON.parse("'a'")));
console.log("parse-unquoted-key=" + probe(() => JSON.parse("{a:1}")));
console.log("parse-undefined=" + probe(() => JSON.parse("undefined")));
console.log("parse-nan=" + probe(() => JSON.parse("NaN")));
console.log("parse-leading-plus=" + probe(() => JSON.parse("+1")));
console.log("parse-hex=" + probe(() => JSON.parse("0x10")));
console.log("parse-comment=" + probe(() => JSON.parse("1 // tail")));
console.log("parse-truncated=" + probe(() => JSON.parse('{"a":')));
console.log("parse-bad-escape=" + probe(() => JSON.parse('"\\x41"')));
console.log("parse-lone-surrogate-ok=" + probe(() => JSON.parse('"\\ud800"').length));
console.log("parse-number-ok=" + probe(() => JSON.parse("1e3")));
console.log("parse-null-ok=" + probe(() => JSON.parse("null")));
console.log("parse-whitespace-ok=" + probe(() => JSON.parse(" \t\r\n1 ")));
console.log("parse-coerced-arg=" + probe(() => JSON.parse(1 as any)));
console.log("parse-undefined-arg=" + probe(() => JSON.parse(undefined as any)));

// A SyntaxError from the parser is a real Error with the usual shape.
let parsed: any = null;
try {
  JSON.parse("{");
} catch (e: any) {
  parsed = e;
}
console.log("syntax-instanceof=" + (parsed instanceof SyntaxError) + "," + (parsed instanceof Error));
console.log("syntax-name=" + parsed.name);
console.log("syntax-tag=" + Object.prototype.toString.call(parsed));
console.log("syntax-stack-type=" + typeof parsed.stack);
console.log("syntax-message-type=" + typeof parsed.message);

// stringify: a cycle is a TypeError, however deep or indirect.
const direct: any = {};
direct.self = direct;
console.log("cycle-direct=" + probe(() => JSON.stringify(direct)));

const a: any = { name: "a" };
const b: any = { name: "b", a: a };
a.b = b;
console.log("cycle-indirect=" + probe(() => JSON.stringify(a)));

const arr: any[] = [1];
arr.push(arr);
console.log("cycle-array=" + probe(() => JSON.stringify(arr)));

// The same object twice is NOT a cycle.
const shared: any = { v: 1 };
console.log("shared-twice=" + probe(() => JSON.stringify({ x: shared, y: shared })));

// BigInt has no JSON representation: TypeError, unless a toJSON is supplied.
console.log("bigint-value=" + probe(() => JSON.stringify(1n as any)));
console.log("bigint-nested=" + probe(() => JSON.stringify({ v: 2n } as any)));
console.log("bigint-replaced=" + probe(() => JSON.stringify({ v: 3n } as any, (k: string, x: any) => (typeof x === "bigint" ? String(x) : x))));

// A throwing getter, toJSON or replacer surfaces the user's own error object,
// identity preserved.
const marker = new RangeError("mine-getter");
const throwingGetter: any = {
  get boom(): any {
    throw marker;
  },
};
let caughtSame = "none";
try {
  JSON.stringify(throwingGetter);
} catch (e: any) {
  caughtSame = (e === marker) + ":" + e.constructor.name + ":" + e.message;
}
console.log("getter-identity=" + caughtSame);

class BadJson {
  toJSON(): any {
    throw new EvalError("mine-tojson");
  }
}
console.log("tojson-throws=" + probe(() => JSON.stringify(new BadJson())));
console.log("replacer-throws=" + probe(() => JSON.stringify({ a: 1 }, () => {
  throw new URIError("mine-replacer");
})));
console.log("reviver-throws=" + probe(() => JSON.parse('{"a":1}', () => {
  throw new ReferenceError("mine-reviver");
})));

// A non-callable replacer or reviver is simply ignored, not an error.
console.log("replacer-number=" + probe(() => JSON.stringify({ a: 1 }, 5 as any)));
console.log("reviver-number=" + probe(() => JSON.parse('{"a":1}', 5 as any).a));

// A bad `space` argument is clamped, never refused.
console.log("space-huge=" + probe(() => JSON.stringify({ a: 1 }, null, 100).length));
console.log("space-negative=" + probe(() => JSON.stringify({ a: 1 }, null, -1)));
console.log("space-string=" + probe(() => JSON.stringify({ a: 1 }, null, "..")));

// A toJSON that is not a function is ignored rather than called.
console.log("tojson-not-callable=" + probe(() => JSON.stringify({ toJSON: 5, a: 1 })));

// A Symbol value disappears; a Symbol as a top-level value yields undefined.
console.log("symbol-value=" + probe(() => String(JSON.stringify(Symbol("s") as any))));
console.log("symbol-in-object=" + probe(() => JSON.stringify({ s: Symbol("s") as any, k: 1 })));
console.log("symbol-in-array=" + probe(() => JSON.stringify([Symbol("s") as any])));
console.log("function-value=" + probe(() => String(JSON.stringify(function () { return 1; } as any))));
console.log("undefined-value=" + probe(() => String(JSON.stringify(undefined as any))));
