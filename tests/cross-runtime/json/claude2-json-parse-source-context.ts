// Cross-runtime: the reviver's THIRD argument. For a primitive it carries the
// exact source text the value was parsed from, which is the only way to see a
// number's original digits after they have been rounded into a double; for an
// object or an array it carries nothing, because no single span describes them.

// --- a number keeps its source text, digit for digit ---
function sourceOf(text: string): string {
  let out = "MISSING";
  JSON.parse(text, function (key: string, value: any, context: any) {
    if (key === "") out = "value=" + String(value) + ",source=" + JSON.stringify(context.source);
    return value;
  } as any);
  return out;
}
console.log("integer=" + sourceOf("1"));
console.log("trailing_zero=" + sourceOf("1.0"));
console.log("trailing_zeros=" + sourceOf("1.500"));
console.log("exponent=" + sourceOf("1e2"));
console.log("exponent_plus=" + sourceOf("1E+2"));
console.log("negative_zero=" + sourceOf("-0"));
console.log("precision_lost=" + sourceOf("12345678901234567890"));
console.log("tiny=" + sourceOf("1e-400"));
console.log("huge=" + sourceOf("1e400"));
console.log("string=" + sourceOf('"ab"'));
console.log("escaped_string=" + sourceOf('"a\\u0041"'));
console.log("true=" + sourceOf("true"));
console.log("null=" + sourceOf("null"));

// --- surrounding whitespace is not part of the value's source ---
console.log("padded=" + sourceOf("  1  "));

// --- an object or an array has NO source ---
function contextOf(text: string): string {
  let out = "MISSING";
  JSON.parse(text, function (key: string, value: any, context: any) {
    if (key === "") {
      out = "type=" + typeof context +
        ",has_source=" + ("source" in context) +
        ",source=" + String(context.source) +
        ",keys=" + Reflect.ownKeys(context).map(String).join("/");
    }
    return value;
  } as any);
  return out;
}
console.log("object_context=" + contextOf('{"a":1}'));
console.log("array_context=" + contextOf("[1]"));
console.log("empty_object_context=" + contextOf("{}"));
console.log("primitive_context=" + contextOf("7"));

// --- the context object's own shape ---
let ctxShape = "";
JSON.parse("7", function (key: string, value: any, context: any) {
  if (key === "") {
    const d: any = Object.getOwnPropertyDescriptor(context, "source");
    ctxShape = "proto_is_object=" + (Object.getPrototypeOf(context) === Object.prototype) +
      ",flags=" + d.writable + d.enumerable + d.configurable +
      ",extensible=" + Object.isExtensible(context) +
      ",json=" + JSON.stringify(context);
  }
  return value;
} as any);
console.log("context_shape=" + ctxShape);

// --- a fresh context object per visit ---
const seenContexts: any[] = [];
JSON.parse("[1,2]", function (key: string, value: any, context: any) {
  seenContexts.push(context);
  return value;
} as any);
console.log("context_count=" + seenContexts.length);
console.log("contexts_distinct=" + (seenContexts[0] === seenContexts[1]));
console.log("context_sources=" + seenContexts.map((c: any) => String(c.source)).join(","));

// --- inside a structure, each primitive still carries its own text ---
const rows: string[] = [];
JSON.parse('{"a":1.0,"b":[2e0,"s"],"c":{"d":3}}', function (key: string, value: any, context: any) {
  rows.push(key + "=" + ("source" in context ? JSON.stringify(context.source) : "-"));
  return value;
} as any);
console.log("structure=" + rows.join("|"));

// --- the argument count the reviver actually receives ---
let argCount = -1;
JSON.parse('{"a":1}', function () { argCount = arguments.length; return arguments[1]; } as any);
console.log("reviver_arity=" + argCount);

// --- the source text is exactly what round-trips: a big number can be kept ---
const bigText = '{"id":9007199254740993,"n":1}';
const kept: any = JSON.parse(bigText, function (key: string, value: any, context: any) {
  if (key === "id") return "RAW:" + context.source;
  return value;
} as any);
console.log("preserved=" + kept.id);
console.log("lossy_default=" + JSON.parse(bigText).id);
console.log("source_differs_from_value=" + (kept.id !== "RAW:" + String(JSON.parse(bigText).id)));

// --- a value replaced by the reviver still reports its ORIGINAL source ---
const replaced: string[] = [];
JSON.parse("[1,2]", function (key: string, value: any, context: any) {
  if (key !== "") replaced.push(key + ":" + String(context.source));
  return key === "" ? value : 999;
} as any);
console.log("original_sources=" + replaced.join(","));

// --- and the context is present even when the reviver ignores it ---
console.log("plain_reviver=" + JSON.stringify(JSON.parse("[1,2]", (k, v) => v)));

// --- an object built by a reviver has no source for the parent, only children ---
const parentRows: string[] = [];
JSON.parse('{"outer":{"inner":5}}', function (this: any, key: string, value: any, context: any) {
  parentRows.push(key + "|holder_keys=" + Object.keys(this).join("/") + "|source=" + ("source" in context ? String(context.source) : "-"));
  return value;
} as any);
console.log("parent_rows=" + parentRows.join(" ;; "));
