// Pins a proxy over a TYPED ARRAY: the index properties forward as ordinary
// string keys, but everything that needs the [[TypedArrayName]] slot — length,
// the iterator, ArrayBuffer.isView, the @@toStringTag getter — refuses the
// proxy, so the transparency is only skin deep.

function attempt(label: string, fn: () => string): void {
  try {
    console.log(label + "=" + fn());
  } catch (e: any) {
    console.log(label + "=throw:" + e.constructor.name);
  }
}

const ta = new Uint8Array([10, 20, 30]);
const log: string[] = [];
const p: any = new Proxy(ta, {
  get(t, k, r) { log.push("get:" + String(k)); return Reflect.get(t, k, r); },
  set(t, k, v, r) { log.push("set:" + String(k)); return Reflect.set(t, k, v, r); },
  has(t, k) { log.push("has:" + String(k)); return Reflect.has(t, k); },
  ownKeys(t) { log.push("ownKeys"); return Reflect.ownKeys(t); },
});

log.length = 0;
console.log("index_read=" + p[1] + "|" + log.join(","));
log.length = 0;
console.log("index_write=" + Reflect.set(p, "0", 99) + ",target=" + ta[0] + "|" + log.join(","));
log.length = 0;
console.log("out_of_range_write=" + Reflect.set(p, "9", 1) + ",len=" + ta.length + "|" + log.join(","));
log.length = 0;
console.log("ownKeys=" + Reflect.ownKeys(p).join("|") + "|" + log.join(","));
console.log("keys=" + Object.keys(p).join("|"));
console.log("has_index=" + ("2" in p) + ",has_missing=" + ("7" in p));

// the accessors on %TypedArray%.prototype all require the slot
attempt("length", () => String(p.length));
attempt("byteLength", () => String(p.byteLength));
attempt("byteOffset", () => String(p.byteOffset));
attempt("buffer", () => String(typeof p.buffer));
attempt("iterate", () => { let n = 0; for (const _v of p) n++; return String(n); });
attempt("join", () => String(p.join("-")));
attempt("subarray", () => String(p.subarray(0, 1).length));
attempt("set_method", () => { p.set([1]); return "ok"; });
attempt("array_from", () => Array.from(p as any).join("-"));

// but the generic Array.prototype methods, which only use length and indices,
// go through the traps happily once length is supplied by the trap
const lengthed: any = new Proxy(ta, { get(t, k, r) { return k === "length" ? (t as any).length : Reflect.get(t, k, r); } });
console.log("generic_join=" + Array.prototype.join.call(lengthed, "-"));
console.log("generic_slice=" + (Array.prototype.slice.call(lengthed) as any[]).join("-"));
console.log("generic_map=" + (Array.prototype.map.call(lengthed, (v: any) => v + 1) as any[]).join("-"));

// the structural predicates do not pierce the proxy
console.log("isView=" + ArrayBuffer.isView(p));
console.log("isView_target=" + ArrayBuffer.isView(ta));
console.log("isArray=" + Array.isArray(p));
console.log("tag=" + Object.prototype.toString.call(p));
console.log("tag_target=" + Object.prototype.toString.call(ta));
console.log("instanceof=" + (p instanceof Uint8Array));
console.log("proto=" + (Object.getPrototypeOf(p) === Uint8Array.prototype));

// integer-indexed exotic behaviour still belongs to the target: an out-of-range
// index is not created, and a canonical-numeric string is refused for define
console.log("define_index=" + Reflect.defineProperty(p, "1", { value: 7, configurable: true, writable: true, enumerable: true }));
console.log("define_index_value=" + ta[1]);
console.log("define_out_of_range=" + Reflect.defineProperty(p, "5", { value: 7, configurable: true, writable: true, enumerable: true }));
console.log("delete_index=" + Reflect.deleteProperty(p, "0") + ",still=" + ta[0]);
console.log("named_prop=" + Reflect.set(p, "note", "N") + ",read=" + p.note);
console.log("json=" + JSON.stringify(p));
console.log("json_target=" + JSON.stringify(ta));

// a DataView behind a proxy fails the same way
const dv = new DataView(new ArrayBuffer(4));
dv.setUint8(0, 42);
const pdv: any = new Proxy(dv, {});
attempt("dataview_get", () => String(pdv.getUint8(0)));
console.log("dataview_via_target=" + Reflect.apply(pdv.getUint8, dv, [0]));
console.log("dataview_isView=" + ArrayBuffer.isView(pdv));
