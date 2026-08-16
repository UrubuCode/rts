// Pins JSON.stringify over proxies: the array/object decision is IsArray, which
// pierces the proxy, so an array target is walked by length and index (no
// ownKeys at all) while an object target is walked by ownKeys — and a callable
// target is skipped entirely.

const log: string[] = [];

function traced(target: any): any {
  return new Proxy(target, {
    get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
    ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
    has(t, k) { log.push("has:" + String(k)); return Reflect.has(t, k); },
  });
}

function run(label: string, target: any, replacer?: any, space?: any): void {
  log.length = 0;
  const out = JSON.stringify(traced(target), replacer, space);
  console.log(label + "=" + String(out) + "|" + log.join(","));
}

run("object", { a: 1, b: "x" });
run("array", [1, 2]);
run("nested_array", [[1], 2]);
run("empty_array", []);
run("sparse_array", [1, , 3]);
run("function", function f() { return 1; });
run("with_undefined", { a: undefined, b: 1 });
run("with_symbol_key", { a: 1, [Symbol("s")]: 2 });
run("nonenumerable", Object.defineProperty({ a: 1 }, "hidden", { value: 2, enumerable: false }));
run("with_replacer_array", { a: 1, b: 2, c: 3 }, ["b", "a"]);
run("with_space", { a: 1 }, undefined, 2);

// toJSON is fetched through the get trap and its result is serialised instead
const withToJSON: any = { a: 1, toJSON() { return { replaced: true }; } };
run("tojson", withToJSON);

// a get trap can supply toJSON for a target that has none
log.length = 0;
const injected: any = new Proxy({ a: 1 }, {
  get(t, k, r) { log.push("get:" + String(k)); return k === "toJSON" ? () => "INJECTED" : Reflect.get(t, k, r); },
  ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
  getOwnPropertyDescriptor(t, k) { return Reflect.getOwnPropertyDescriptor(t, k); },
});
console.log("injected=" + JSON.stringify(injected) + "|" + log.join(","));

// a non-callable toJSON is ignored rather than a TypeError
console.log("tojson_notfn=" + JSON.stringify(new Proxy({ a: 1 }, { get(t, k, r) { return k === "toJSON" ? 5 : Reflect.get(t, k, r); } })));

// the ownKeys trap decides the object's shape; gopd decides enumerability
const invented: any = new Proxy({ real: 1 } as any, {
  ownKeys() { return ["real", "ghost"]; },
  getOwnPropertyDescriptor(t, k) {
    if (k === "ghost") return { value: 0, enumerable: true, configurable: true, writable: true };
    return Reflect.getOwnPropertyDescriptor(t, k);
  },
  get(t, k, r) { return k === "ghost" ? "G" : Reflect.get(t, k, r); },
});
console.log("invented=" + JSON.stringify(invented));
const hidden: any = new Proxy({ a: 1, b: 2 }, {
  getOwnPropertyDescriptor(t, k) {
    const d = Reflect.getOwnPropertyDescriptor(t, k) as any;
    if (k === "b" && d) d.enumerable = false;
    return d;
  },
});
console.log("hidden=" + JSON.stringify(hidden));

// an array proxy ignores ownKeys entirely: length and the indices are all that
// is read
const lyingKeys: any = new Proxy([1, 2, 3], { ownKeys() { return ["0"]; } });
console.log("array_ignores_ownKeys=" + JSON.stringify(lyingKeys));
const lyingLength: any = new Proxy([1, 2, 3], { get(t, k, r) { return k === "length" ? 1 : Reflect.get(t, k, r); } });
console.log("array_follows_length=" + JSON.stringify(lyingLength));

// proxies nested inside ordinary structures serialise recursively
console.log("nested_mixed=" + JSON.stringify({ arr: new Proxy([1, new Proxy({ k: 2 }, {})], {}), obj: new Proxy({ z: 3 }, {}) }));

// the replacer is called with the proxy as `this` holder for its keys
const holders: string[] = [];
const holderProxy: any = new Proxy({ a: 1, b: 2 }, {});
JSON.stringify(holderProxy, function (this: any, k: string, v: any) { holders.push(k + ":" + (this === holderProxy)); return v; });
console.log("replacer_holder=" + holders.join(","));

// a proxy of a boxed primitive is NOT unwrapped: the [[NumberData]] check is a
// slot test, and a proxy has no slots — so it takes the ordinary object path
console.log("boxed_number=" + JSON.stringify(new Proxy(new Number(7), {})));
console.log("boxed_string=" + JSON.stringify(new Proxy(new String("s"), {})));
console.log("boxed_bool=" + JSON.stringify(new Proxy(new Boolean(true), {})));
