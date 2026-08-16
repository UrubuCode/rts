// Cross-runtime: structuredClone over the BINARY and BOXED types — buffers and
// views (and the aliasing between them), boxed primitives, BigInt, -0, Error
// subclasses, and a Map/Set whose keys are themselves clonable objects.

// A view is cloned as a view over a cloned buffer.
const src = new ArrayBuffer(8);
const bytes = new Uint8Array(src);
bytes.set([1, 2, 3, 4, 5, 6, 7, 8]);
const clonedBuffer = structuredClone(src);
console.log("buffer=" + clonedBuffer.byteLength + " detached_src=" + src.detached + " same=" + (clonedBuffer === src));
console.log("buffer_bytes=" + Array.from(new Uint8Array(clonedBuffer)).join(","));
new Uint8Array(clonedBuffer)[0] = 99;
console.log("buffer_independent=" + bytes[0]);

const view = new Int16Array(src, 2, 2);
const clonedView = structuredClone(view);
console.log("view_kind=" + clonedView.constructor.name + " len=" + clonedView.length + " byteOffset=" + clonedView.byteOffset + " bufferLen=" + clonedView.buffer.byteLength);

// Two views over ONE buffer keep their aliasing after the clone.
const aliasA = new Uint8Array(src, 0, 4);
const aliasB = new Uint8Array(src, 2, 4);
const pair = structuredClone({ a: aliasA, b: aliasB });
console.log("alias_same_buffer=" + (pair.a.buffer === pair.b.buffer) + " offsets=" + pair.a.byteOffset + "," + pair.b.byteOffset);
pair.a[2] = 77;
console.log("alias_visible=" + pair.b[0]);

// Every element kind survives, including the BigInt ones.
const kinds: any[] = [
  new Int8Array([-1]),
  new Uint8Array([255]),
  new Uint8ClampedArray([300]),
  new Int16Array([-2]),
  new Uint16Array([65535]),
  new Int32Array([-3]),
  new Uint32Array([4294967295]),
  new Float32Array([1.5]),
  new Float64Array([-0]),
  new BigInt64Array([-1n]),
  new BigUint64Array([1n]),
];
for (const k of kinds) {
  const c = structuredClone(k);
  console.log("kind_" + k.constructor.name + "=" + c.constructor.name + ":" + Array.from(c).join(","));
}
console.log("float64_negzero=" + Object.is(structuredClone(new Float64Array([-0]))[0], -0));

const dv = new DataView(new ArrayBuffer(4), 1, 2);
const clonedDv = structuredClone(dv);
console.log("dataview=" + clonedDv.constructor.name + " off=" + clonedDv.byteOffset + " len=" + clonedDv.byteLength + " bufferLen=" + clonedDv.buffer.byteLength);

// A resizable buffer keeps its maxByteLength.
const rb = new ArrayBuffer(2, { maxByteLength: 8 });
const clonedRb = structuredClone(rb);
console.log("resizable=" + clonedRb.resizable + " max=" + clonedRb.maxByteLength + " len=" + clonedRb.byteLength);

// Boxed primitives clone as boxed objects, not as primitives.
const boxedNumber: any = structuredClone(new Number(5) as any);
const boxedString: any = structuredClone(new String("s") as any);
const boxedBoolean: any = structuredClone(new Boolean(true) as any);
console.log("boxed_number=" + typeof boxedNumber + " " + boxedNumber.valueOf() + " " + (boxedNumber instanceof Number));
console.log("boxed_string=" + typeof boxedString + " " + boxedString.valueOf() + " len=" + boxedString.length);
console.log("boxed_boolean=" + typeof boxedBoolean + " " + boxedBoolean.valueOf());
console.log("boxed_tags=" + Object.prototype.toString.call(boxedNumber) + Object.prototype.toString.call(boxedString));

// Primitives pass through unchanged, including -0, BigInt and NaN.
console.log("undefined=" + String(structuredClone(undefined)));
console.log("null=" + String(structuredClone(null)));
console.log("negzero=" + Object.is(structuredClone(-0), -0));
console.log("nan=" + Number.isNaN(structuredClone(NaN)));
console.log("bigint=" + structuredClone(2n ** 70n) + " typeof=" + typeof structuredClone(1n));
console.log("big_string=" + structuredClone("é\u{1F600}").length);
console.log("infinity=" + structuredClone(Infinity) + "," + structuredClone(-Infinity));

// The seven ES error types keep their constructor, name and message; anything
// outside that list — a subclass, AggregateError — flattens to plain Error, and
// an extra own property is not carried across.
const errors: Error[] = [
  new Error("e"),
  new TypeError("t"),
  new RangeError("r"),
  new SyntaxError("s"),
  new ReferenceError("f"),
  new EvalError("v"),
  new URIError("u"),
];
for (const e of errors) {
  const c: any = structuredClone(e);
  console.log("error_" + e.constructor.name + "=" + c.constructor.name + "/" + c.name + "/" + c.message + " stack=" + typeof c.stack);
}
const agg: any = structuredClone(new AggregateError([new Error("a"), new RangeError("b")], "many"));
console.log("aggregate=" + agg.constructor.name + " msg=" + agg.message + " name=" + agg.name);
class MyError extends Error {}
const custom: any = structuredClone(new MyError("custom"));
console.log("subclass_flattened=" + custom.constructor.name + " msg=" + custom.message);
const extra: any = new Error("x");
extra.code = 42;
console.log("error_extra_prop=" + String(structuredClone(extra).code));

// Map and Set with object keys keep identity within the clone.
const key = { id: 1 };
const m = new Map<any, any>([[key, "v"], ["s", key]]);
const mc = structuredClone(m);
const clonedKey = [...mc.keys()][0];
console.log("map_key_identity=" + (mc.get(clonedKey) === "v") + " value_is_same_key=" + (mc.get("s") === clonedKey));
console.log("map_key_is_copy=" + (clonedKey !== key) + " size=" + mc.size);
const s = new Set([key, { id: 2 }]);
const sc = structuredClone(s);
console.log("set_size=" + sc.size + " ids=" + [...sc].map(function (x: any) { return x.id; }).join(","));
const nested = structuredClone(new Map([["inner", new Set([new Date(0)])]]));
console.log("nested=" + ((nested.get("inner") as Set<Date>).size) + " " + [...(nested.get("inner") as Set<Date>)][0].toISOString());

// Date and RegExp keep their state, and lastIndex is reset.
const re = /ab+c/giu;
re.lastIndex = 3;
const rc = structuredClone(re);
console.log("regexp=" + rc.source + "/" + rc.flags + " lastIndex=" + rc.lastIndex);
const invalidDate = structuredClone(new Date(NaN));
console.log("invalid_date=" + (invalidDate instanceof Date) + " " + Number.isNaN(invalidDate.getTime()));
