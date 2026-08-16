// Cross-runtime: the TypeErrors the collection and wrapper constructors raise
// about their ARGUMENTS — a non-iterable, an iterable whose elements are not
// entry pairs, a non-object WeakMap key, a non-callable Promise executor — and
// the argument shapes each one quietly accepts instead.
function probe(fn: () => any): string {
  try {
    const v = fn();
    return "ok:" + String(v);
  } catch (e: any) {
    return e.constructor.name;
  }
}

// Map/Set accept null and undefined as "no entries", but refuse anything else
// that is not iterable.
console.log("map-null=" + probe(() => new Map(null as any).size));
console.log("map-undefined=" + probe(() => new Map(undefined).size));
console.log("map-number=" + probe(() => new Map(5 as any)));
console.log("map-object=" + probe(() => new Map({} as any)));
console.log("map-string=" + probe(() => new Map("ab" as any)));
console.log("set-null=" + probe(() => new Set(null as any).size));
console.log("set-number=" + probe(() => new Set(5 as any)));
console.log("set-string=" + probe(() => new Set("aab").size));
console.log("set-arraylike=" + probe(() => new Set({ length: 2 } as any)));

// Map entries must be objects with 0 and 1; a primitive element is refused.
console.log("map-entry-number=" + probe(() => new Map([1] as any)));
console.log("map-entry-null=" + probe(() => new Map([null] as any)));
console.log("map-entry-string=" + probe(() => new Map(["ab"] as any).get("a")));
console.log("map-entry-short=" + probe(() => new Map([["k"]] as any).get("k")));
console.log("map-entry-long=" + probe(() => new Map([["k", "v", "extra"]] as any).get("k")));
console.log("map-entry-object=" + probe(() => new Map([{ 0: "k", 1: "v" }] as any).get("k")));

// A Symbol.iterator that is present but not callable.
const badIterable: any = { [Symbol.iterator]: 5 };
console.log("map-bad-iterator=" + probe(() => new Map(badIterable)));
console.log("set-bad-iterator=" + probe(() => new Set(badIterable)));

// An iterator whose `next` answers a PRIMITIVE lives in
// iteration/claude2-malformed-iterator-results.ts. It is kept apart because an
// engine that does not check the result's type never sees `done` and loops
// forever, and a hang here would hide everything below it.

// WeakMap and WeakSet demand objects (and, since ES2023, symbols) as keys.
console.log("weakmap-number-key=" + probe(() => new WeakMap([[1, "v"]] as any)));
console.log("weakmap-set-number=" + probe(() => new WeakMap().set(1 as any, "v")));
console.log("weakmap-object-ok=" + probe(() => new WeakMap().set({}, "v").constructor.name));
console.log("weakset-number=" + probe(() => new WeakSet().add(1 as any)));
console.log("weakset-object-ok=" + probe(() => new WeakSet().add({}).constructor.name));
console.log("weakmap-get-primitive=" + probe(() => String(new WeakMap().get(1 as any))));
console.log("weakmap-has-primitive=" + probe(() => new WeakMap().has(1 as any)));

// The Promise executor must be callable, and is called synchronously.
console.log("promise-no-executor=" + probe(() => new (Promise as any)()));
console.log("promise-number=" + probe(() => new (Promise as any)(5)));
console.log("promise-object=" + probe(() => new (Promise as any)({})));
const executed: string[] = [];
const pr = new Promise<number>((res) => {
  executed.push("sync");
  res(1);
});
console.log("promise-sync=" + executed.join(",") + ":" + (pr instanceof Promise));

// A throwing executor rejects rather than propagating.
console.log("promise-throwing-executor=" + probe(() => {
  const failing = new Promise(() => {
    throw new RangeError("exec");
  });
  failing.catch(() => undefined);
  return failing instanceof Promise;
}));

// Proxy demands two objects.
console.log("proxy-no-args=" + probe(() => new (Proxy as any)()));
console.log("proxy-primitive-target=" + probe(() => new Proxy(5 as any, {})));
console.log("proxy-primitive-handler=" + probe(() => new Proxy({}, 5 as any)));
console.log("proxy-null-handler=" + probe(() => new Proxy({}, null as any)));
console.log("proxy-ok=" + probe(() => typeof new Proxy({}, {})));
console.log("proxy-revocable=" + probe(() => typeof Proxy.revocable({}, {}).revoke));

// DataView and typed arrays over a non-buffer.
console.log("dataview-object=" + probe(() => new DataView({} as any)));
console.log("dataview-array=" + probe(() => new DataView([] as any)));
console.log("dataview-ok=" + probe(() => new DataView(new ArrayBuffer(4)).byteLength));
console.log("typedarray-object-ok=" + probe(() => new Uint8Array({ length: 2 } as any).length));
console.log("typedarray-null=" + probe(() => new Uint8Array(null as any).length));
console.log("typedarray-iterable-ok=" + probe(() => new Uint8Array([1, 2]).length));

// Object.defineProperty and friends on a primitive.
console.log("defineproperty-number=" + probe(() => Object.defineProperty(5 as any, "a", { value: 1 })));
console.log("defineproperties-string=" + probe(() => Object.defineProperties("s" as any, {})));
console.log("getownpropertydescriptor-number-ok=" + probe(() => String(Object.getOwnPropertyDescriptor(5 as any, "a"))));
console.log("keys-number-ok=" + probe(() => Object.keys(5 as any).length));
console.log("keys-null=" + probe(() => Object.keys(null as any)));
console.log("assign-null-target=" + probe(() => Object.assign(null as any, {})));
console.log("assign-null-source-ok=" + probe(() => Object.keys(Object.assign({}, null as any)).length));

// Reflect refuses primitives where Object coerces them.
console.log("reflect-ownkeys-number=" + probe(() => Reflect.ownKeys(5 as any)));
console.log("reflect-get-number=" + probe(() => Reflect.get(5 as any, "a")));
console.log("reflect-setproto-number=" + probe(() => Reflect.setPrototypeOf(5 as any, null)));
console.log("object-setproto-number-ok=" + probe(() => typeof Object.setPrototypeOf(5 as any, null)));

// A brand check on a method borrowed off the wrong prototype.
console.log("map-get-on-set=" + probe(() => Map.prototype.get.call(new Set() as any, "a")));
console.log("set-add-on-map=" + probe(() => Set.prototype.add.call(new Map() as any, "a")));
console.log("promise-then-on-object=" + probe(() => Promise.prototype.then.call({} as any, () => undefined)));
console.log("dataview-getter-on-object=" + probe(() => DataView.prototype.getInt8.call({} as any, 0)));
