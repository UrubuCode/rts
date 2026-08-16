// Cross-runtime: every platform class this corpus reaches is a REAL
// constructor — a function with a wired .prototype whose .constructor points
// back, refusing to be called without new. An engine that installs a global as
// a plain factory object passes nothing here.

const roster: string[] = [
  "URL",
  "URLSearchParams",
  "TextEncoder",
  "TextDecoder",
  "Blob",
  "Headers",
  "Request",
  "Response",
  "FormData",
  "AbortController",
  "AbortSignal",
  "DOMException",
  "Event",
  "EventTarget",
  "MessageChannel",
  "MessagePort",
  "Promise",
  "Map",
  "Set",
  "WeakMap",
  "WeakSet",
  "WeakRef",
  "FinalizationRegistry",
  "ArrayBuffer",
  "SharedArrayBuffer",
  "DataView",
  "BigInt64Array",
];

for (const name of roster) {
  const C: any = (globalThis as any)[name];
  if (typeof C !== "function") {
    console.log("class[" + name + "]=" + typeof C);
    continue;
  }
  const hasProto = C.prototype !== undefined && C.prototype !== null;
  const wired = hasProto && C.prototype.constructor === C;
  let withoutNew = "accepted";
  try {
    C();
  } catch (e: any) {
    withoutNew = e.constructor.name;
  }
  console.log("class[" + name + "]=name:" + C.name + " wired:" + wired + " noNew:" + withoutNew + " protoIsObject:" + (typeof C.prototype === "object"));
}

// Proxy is the odd one: a constructor with NO prototype property at all.
console.log("proxy_no_prototype=" + (Proxy.prototype === undefined) + " name=" + Proxy.name + " len=" + Proxy.length);
console.log("proxy_instanceof=" + (function (): string {
  try {
    return String(({} as any) instanceof Proxy);
  } catch (e: any) {
    return e.constructor.name;
  }
})());

// The arities that are the same everywhere.
const arities: string[] = ["URL", "URLSearchParams", "TextEncoder", "TextDecoder", "Blob", "Headers", "Response", "AbortController", "DOMException", "Event", "EventTarget", "Promise", "Map", "Set", "WeakRef", "ArrayBuffer", "DataView"];
console.log("arity=" + arities.map(function (n) {
  return n + ":" + (globalThis as any)[n].length;
}).join(" "));

// A prototype method is on the PROTOTYPE, never an own property of an instance.
const owned: Array<[string, unknown, string]> = [
  ["URL", new URL("https://e.example/"), "toString"],
  ["URLSearchParams", new URLSearchParams("a=1"), "append"],
  ["Headers", new Headers(), "append"],
  ["Blob", new Blob(), "slice"],
  ["TextEncoder", new TextEncoder(), "encode"],
  ["TextDecoder", new TextDecoder(), "decode"],
  ["Map", new Map(), "get"],
  ["AbortController", new AbortController(), "abort"],
];
for (const o of owned) {
  const instance: any = o[1];
  const key = o[2];
  console.log("method[" + o[0] + "." + key + "]=own:" + Object.prototype.hasOwnProperty.call(instance, key) + " onProto:" + Object.prototype.hasOwnProperty.call(Object.getPrototypeOf(instance), key) + " callable:" + (typeof instance[key] === "function"));
}

// Instances answer to instanceof and carry no own enumerable state.
const instances: Array<[string, unknown]> = [
  ["URL", new URL("https://e.example/")],
  ["URLSearchParams", new URLSearchParams("a=1")],
  ["Headers", new Headers({ a: "1" })],
  ["Blob", new Blob(["x"])],
  ["Response", new Response()],
  ["AbortController", new AbortController()],
  ["DOMException", new DOMException("m", "AbortError")],
  ["EventTarget", new EventTarget()],
];
for (const i of instances) {
  const v: any = i[1];
  const C: any = (globalThis as any)[i[0]];
  console.log("instance[" + i[0] + "]=is:" + (v instanceof C) + " ctor:" + (v.constructor === C) + " ownKeys:" + Object.keys(v).length);
}

// Subclassing works through the ordinary [[Construct]] chain.
class TaggedURL extends URL {
  tag = "t";
}
const sub = new TaggedURL("https://e.example/p");
console.log("subclass=" + sub.pathname + " tag=" + sub.tag + " isURL=" + (sub instanceof URL) + " isSub=" + (sub instanceof TaggedURL));
console.log("subclass_proto=" + (Object.getPrototypeOf(TaggedURL.prototype) === URL.prototype) + " ctor_chain=" + (Object.getPrototypeOf(TaggedURL) === URL));

class TaggedHeaders extends Headers {}
const subHeaders = new TaggedHeaders({ a: "1" });
console.log("subclass_headers=" + subHeaders.get("a") + " isHeaders=" + (subHeaders instanceof Headers) + " ctorName=" + subHeaders.constructor.name);

// Reflect.construct against a different new.target rewires the prototype.
const viaReflect: any = Reflect.construct(URL, ["https://e.example/z"], TaggedURL);
console.log("reflect_construct=" + viaReflect.pathname + " isSub=" + (viaReflect instanceof TaggedURL) + " hasField=" + ("tag" in viaReflect));

// Symbol.hasInstance is not overridden on any of them.
console.log("no_custom_hasInstance=" + roster.filter(function (n) {
  const C: any = (globalThis as any)[n];
  return typeof C === "function" && Object.prototype.hasOwnProperty.call(C, Symbol.hasInstance);
}).length);
