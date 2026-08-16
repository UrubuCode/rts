// Pins destructuring against a proxy: object patterns are plain [[Get]]s (one
// per named binding, no ownKeys), a REST element switches to
// CopyDataProperties — ownKeys, then a descriptor and a get per key it keeps —
// and an array pattern goes through the iterator protocol.

const log: string[] = [];

function traced(target: any): any {
  return new Proxy(target, {
    get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
    has(t, k) { log.push("has:" + String(k)); return Reflect.has(t, k); },
    ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
    getOwnPropertyDescriptor(t, k) { log.push("gopd:" + String(k)); return Reflect.getOwnPropertyDescriptor(t, k); },
  });
}

function run(label: string, fn: (p: any) => string): void {
  log.length = 0;
  const src: any = { a: 1, b: 2, c: 3 };
  Object.defineProperty(src, "hidden", { value: 4, enumerable: false, configurable: true });
  let out: string;
  try {
    out = fn(traced(src));
  } catch (e: any) {
    out = "throw:" + e.constructor.name;
  }
  console.log(label + "=" + out + "|" + log.join(","));
}

run("named", (p) => { const { a, b } = p; return a + "/" + b; });
run("renamed", (p) => { const { a: x, c: y } = p; return x + "/" + y; });
run("missing", (p) => { const { zz } = p; return String(zz); });
run("default_unused", (p) => { const { a = 99 } = p; return String(a); });
run("default_used", (p) => { const { zz = 99 } = p; return String(zz); });
run("computed", (p) => { const key = "b"; const { [key]: v } = p; return String(v); });
run("nested", (p) => { const { a, ...rest } = p; return a + "/" + Object.keys(rest).join("+"); });
run("rest_only", (p) => { const { ...all } = p; return Object.keys(all).join("+"); });
run("rest_after_two", (p) => { const { a, b, ...rest } = p; return a + b + "/" + Object.keys(rest).join("+"); });
run("hidden_named", (p) => { const { hidden } = p as any; return String(hidden); });
run("hidden_in_rest", (p) => { const { ...all } = p; return String((all as any).hidden); });

// the rest object is a plain, extensible object with data properties
const restSource: any = new Proxy({ a: 1, b: 2 }, {});
const { a: _a, ...restObj } = restSource as any;
console.log("rest_proto=" + (Object.getPrototypeOf(restObj) === Object.prototype));
console.log("rest_is_proxy_like=" + (restObj === restSource));
const rd = Object.getOwnPropertyDescriptor(restObj, "b") as any;
console.log("rest_desc=w=" + rd.writable + ",e=" + rd.enumerable + ",c=" + rd.configurable);

// a getter behind the proxy is INVOKED by rest, so the copy holds a data slot
let getterCalls = 0;
const withGetter: any = new Proxy({ get live() { getterCalls++; return "L"; }, plain: 1 }, {});
const { ...copied } = withGetter as any;
console.log("getter_calls=" + getterCalls);
console.log("copied_live=" + (copied as any).live);
console.log("copied_desc=" + (Object.getOwnPropertyDescriptor(copied, "live") as any).get);

// the excluded key is still asked for a descriptor? — no: ownKeys is filtered
// before the descriptor call, so an excluded key is never described
log.length = 0;
const exclusion: any = traced({ keepMe: 1, dropMe: 2 });
const { dropMe: _d, ...kept } = exclusion as any;
console.log("exclusion_log=" + log.join(","));
console.log("kept=" + Object.keys(kept).join("+"));

// array destructuring uses the iterator, not indices
log.length = 0;
const iterable: any = traced([10, 20, 30]);
const [first, second] = iterable as any[];
console.log("array_values=" + first + "/" + second);
console.log("array_log=" + log.join(","));

// an array rest drains the iterator
log.length = 0;
const [head, ...tail] = traced([1, 2, 3]) as any[];
console.log("array_rest=" + head + "/" + tail.join("+"));
console.log("array_rest_log=" + log.join(","));

// a proxy with no @@iterator refuses an array pattern
try {
  const [_x] = new Proxy({ a: 1 } as any, {}) as any;
  console.log("not_iterable=ok");
} catch (e: any) {
  console.log("not_iterable=throw:" + e.constructor.name);
}

// a get trap can supply @@iterator for a target that has none
const fakeIterable: any = new Proxy({} as any, {
  get(_t, k) {
    if (k === Symbol.iterator) return function* () { yield "p"; yield "q"; };
    return undefined;
  },
});
const [p1, p2] = fakeIterable as any[];
console.log("fake_iterable=" + p1 + "/" + p2);

// parameter destructuring behaves the same way
function takes({ a, ...rest }: any): string { return a + "/" + Object.keys(rest).join("+"); }
console.log("param=" + takes(new Proxy({ a: 1, b: 2, c: 3 }, {})));
