// Pins Symbol.toStringTag driving Object.prototype.toString: only a STRING tag
// is honoured, anything else falls back to the builtin tag, and the lookup
// happens through the prototype chain and through a proxy's get trap.
// 425 covers the untagged receivers only.

const ts = Object.prototype.toString;

// a plain object with a string tag
const tagged: any = { [Symbol.toStringTag]: "Custom" };
console.log("string_tag=" + ts.call(tagged));

// non-string tags are ignored
console.log("number_tag=" + ts.call({ [Symbol.toStringTag]: 42 } as any));
console.log("null_tag=" + ts.call({ [Symbol.toStringTag]: null } as any));
console.log("undef_tag=" + ts.call({ [Symbol.toStringTag]: undefined } as any));
console.log("symbol_tag=" + ts.call({ [Symbol.toStringTag]: Symbol("x") } as any));
console.log("object_tag=" + ts.call({ [Symbol.toStringTag]: {} } as any));
console.log("empty_tag=" + ts.call({ [Symbol.toStringTag]: "" } as any));
console.log("weird_tag=" + ts.call({ [Symbol.toStringTag]: "[object Nested]" } as any));

// the builtin tag for an ARRAY is overridden by a string tag
const taggedArr: any = [1, 2];
taggedArr[Symbol.toStringTag] = "NotArray";
console.log("array_tag=" + ts.call(taggedArr));
console.log("array_isarray=" + Array.isArray(taggedArr));
console.log("array_join=" + taggedArr.join("|"));

// a FUNCTION keeps its builtin tag unless tagged
function f(): void { /* noop */ }
console.log("fn_tag=" + ts.call(f));
(f as any)[Symbol.toStringTag] = "Tagged";
console.log("fn_tagged=" + ts.call(f));

// the tag is looked up through the prototype chain
const protoTagged: any = Object.create({ [Symbol.toStringTag]: "Inherited" });
console.log("inherited_tag=" + ts.call(protoTagged));

// a GETTER supplies the tag
const getterTagged: any = {};
Object.defineProperty(getterTagged, Symbol.toStringTag, { get() { return "FromGetter"; } });
console.log("getter_tag=" + ts.call(getterTagged));

// a getter that throws propagates out of toString
const throwing: any = {};
Object.defineProperty(throwing, Symbol.toStringTag, { get(): string { throw new RangeError("t"); } });
try {
  console.log("throwing=" + ts.call(throwing));
} catch (e: any) {
  console.log("throwing=throw:" + e.constructor.name);
}

// null and undefined receivers have their own hardwired answers, no lookup
console.log("null=" + ts.call(null));
console.log("undefined=" + ts.call(undefined));
console.log("no_arg=" + ts.call(undefined as any));

// primitives box to their builtin tags
console.log("number=" + ts.call(1));
console.log("string=" + ts.call("s"));
console.log("boolean=" + ts.call(true));
console.log("bigint=" + ts.call(1n));
console.log("symbol=" + ts.call(Symbol("s")));

// a PROXY answers through the get trap
const proxyTagged: any = new Proxy({}, { get(_t, k) { return k === Symbol.toStringTag ? "ProxyTag" : undefined; } });
console.log("proxy_tag=" + ts.call(proxyTagged));
const proxyOfArray: any = new Proxy([1], {});
console.log("proxy_array=" + ts.call(proxyOfArray));
const proxyOfFn: any = new Proxy(f, { get() { return undefined; } });
console.log("proxy_fn=" + ts.call(proxyOfFn));

// classes: the tag is a normal prototype property
class Tagged {
  get [Symbol.toStringTag](): string { return "MyClass"; }
}
console.log("class_tag=" + ts.call(new Tagged()));
console.log("class_keys=" + Object.keys(new Tagged()).length);
const ctd = Object.getOwnPropertyDescriptor(Tagged.prototype, Symbol.toStringTag) as any;
console.log("class_tag_desc=e=" + ctd.enumerable + ",c=" + ctd.configurable);

// the builtins that ship a tag
console.log("map=" + ts.call(new Map()));
console.log("set=" + ts.call(new Set()));
console.log("promise=" + ts.call(Promise.resolve(1)));
console.log("json=" + ts.call(JSON));
console.log("math=" + ts.call(Math));
console.log("nullproto=" + ts.call(Object.create(null)));

// String(obj) and template interpolation route through toString, not the tag,
// unless the tag is what toString reports
console.log("string_of_tagged=" + String(tagged));
console.log("template=" + `${tagged}`);
console.log("concat=" + ("" + tagged));
