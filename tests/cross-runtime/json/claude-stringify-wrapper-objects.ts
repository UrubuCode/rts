// Cross-runtime: `JSON.stringify` unwraps Boolean/Number/String WRAPPER objects
// to their primitive (SerializeJSONProperty), ignoring expando properties.
//
// Regression guard for issue #2017: `JSON.stringify(new Number(5))` rendered
// `{}` instead of `5`. The engine has TWO wrapper forms — `Object(5)` builds the
// shape object `{ __prim: 5 }`, while `new Number(5)` builds a class instance
// whose primitive is reached through `valueOf()` — and the serializer only knew
// the first, so the second fell through to the plain-object path.

console.log("number=" + JSON.stringify(new Number(5)));
console.log("numberNaN=" + JSON.stringify(new Number(NaN)));
console.log("numberInfinity=" + JSON.stringify(new Number(Infinity)));
console.log("numberNegZero=" + JSON.stringify(new Number(-0)));
console.log("string=" + JSON.stringify(new String("hi")));
console.log("stringQuoted=" + JSON.stringify(new String('a"b')));
console.log("stringEmpty=" + JSON.stringify(new String("")));
console.log("boolTrue=" + JSON.stringify(new Boolean(true)));
console.log("boolFalse=" + JSON.stringify(new Boolean(false)));

console.log("nested=" + JSON.stringify({ a: new Number(7) }));
console.log("inArray=" + JSON.stringify([new Number(1), new String("x"), new Boolean(false)]));
console.log("deep=" + JSON.stringify({ a: { b: [new Number(3)] } }));

// An expando property on a wrapper is IGNORED — the primitive wins.
const withExpando: any = new Number(5);
withExpando.extra = "ignored";
console.log("expandoIgnored=" + JSON.stringify(withExpando));

// ── non-regressions ─────────────────────────────────────────────────────────
console.log("objectWrapperNum=" + JSON.stringify(Object(5)));
console.log("objectWrapperStr=" + JSON.stringify(Object("h")));
console.log("plainObject=" + JSON.stringify({ a: 1 }));
// A plain object that merely HAS a `valueOf` is not a wrapper — it serializes
// as an object (only its own enumerable data properties).
console.log("objectWithValueOf=" + JSON.stringify({ valueOf: () => 9, a: 1 }));
console.log("plainArray=" + JSON.stringify([1, 2]));
console.log("primNumber=" + JSON.stringify(5));
console.log("primString=" + JSON.stringify("hi"));
console.log("primBool=" + JSON.stringify(true));
console.log("primNull=" + JSON.stringify(null));
