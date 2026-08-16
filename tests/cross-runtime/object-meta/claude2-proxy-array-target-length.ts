// Pins a proxy over an ARRAY: the exotic length behaviour belongs to the
// target, so growing and truncating happen through the ordinary set trap, and
// the Array.prototype methods reach the proxy as a plain sequence of get/has.

const log: string[] = [];

function watched(arr: any[]): any {
  return new Proxy(arr, {
    get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
    set(t, k, v, r) { log.push("set:" + String(k) + "=" + String(v)); return Reflect.set(t, k, v, r); },
    has(t, k) { log.push("has:" + String(k)); return Reflect.has(t, k); },
    deleteProperty(t, k) { log.push("del:" + String(k)); return Reflect.deleteProperty(t, k); },
    defineProperty(t, k, d) { log.push("def:" + String(k)); return Reflect.defineProperty(t, k, d); },
  });
}

function run(label: string, fn: (p: any, raw: any[]) => string): void {
  log.length = 0;
  const raw: any[] = [10, 20, 30];
  const p = watched(raw);
  const out = fn(p, raw);
  console.log(label + "=" + out + "|" + log.join(","));
}

run("read_index", (p) => String(p[1]));
run("read_length", (p) => String(p.length));
run("write_in_range", (p, raw) => { Reflect.set(p, "0", 99); return String(raw[0]); });
run("write_past_end", (p, raw) => { Reflect.set(p, "5", 1); return raw.length + ":" + String(raw[4]); });
run("shrink_length", (p, raw) => { Reflect.set(p, "length", 1); return raw.join("/"); });
run("delete_index", (p, raw) => { Reflect.deleteProperty(p, "1"); return raw.length + ":" + String(raw[1]); });
run("push", (p, raw) => { Array.prototype.push.call(p, 40); return raw.join("/"); });
run("pop", (p, raw) => { const v = Array.prototype.pop.call(p); return v + ":" + raw.join("/"); });
run("join", (p) => Array.prototype.join.call(p, "-"));
run("index_of", (p) => String(Array.prototype.indexOf.call(p, 20)));
run("for_of", (p) => { const acc: number[] = []; for (const v of p) acc.push(v as number); return acc.join("/"); });
run("spread_array", (p) => [...(p as number[])].join("/"));
run("concat", (p) => (([] as any[]).concat(p as any) as any[]).join("/"));

// a hole in the target shows through has, and the trap can invent it
const holed: any = [1, , 3];
const holeProxy: any = new Proxy(holed, {
  has(t, k) { return k === "1" ? true : Reflect.has(t, k); },
  get(t, k, r) { return k === "1" ? "INVENTED" : Reflect.get(t, k, r); },
});
console.log("hole_has=" + ("1" in holeProxy) + ",value=" + holeProxy[1]);
console.log("hole_map=" + (Array.prototype.map.call(holeProxy, (v: any) => String(v)) as any[]).join("/"));
console.log("hole_target_map=" + holed.map((v: any) => String(v)).length);

// length reported by a lying get trap drives the generic methods
const liar: any = new Proxy([1, 2, 3, 4, 5], { get(t, k, r) { return k === "length" ? 2 : Reflect.get(t, k, r); } });
console.log("lie_length=" + liar.length);
console.log("lie_join=" + Array.prototype.join.call(liar, ","));
console.log("lie_slice=" + (Array.prototype.slice.call(liar) as any[]).join(","));
console.log("lie_isArray=" + Array.isArray(liar));
console.log("lie_ownKeys=" + Reflect.ownKeys(liar).join("|"));

// the array exotic [[DefineOwnProperty]] still guards length: refusing to
// shrink a non-configurable element is the TARGET's rule, not the proxy's
const guarded: any = [1, 2, 3];
Object.defineProperty(guarded, "1", { value: 2, configurable: false, writable: true, enumerable: true });
const guardProxy: any = new Proxy(guarded, {});
console.log("shrink_guarded=" + Reflect.set(guardProxy, "length", 0));
console.log("guarded_len=" + guarded.length + ",contents=" + guarded.join("/"));

// a proxy of an array is not an array for the purposes of its own methods'
// species lookups, but the result of slice is a plain array
const sliced: any = Array.prototype.slice.call(new Proxy([1, 2, 3], {}));
console.log("slice_is_array=" + Array.isArray(sliced) + ",proto=" + (Object.getPrototypeOf(sliced) === Array.prototype));
console.log("nested_json=" + JSON.stringify([new Proxy([1, 2], {}), new Proxy({ a: 1 }, {})]));
