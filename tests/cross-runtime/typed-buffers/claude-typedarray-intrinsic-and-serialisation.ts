// Cross-runtime: the %TypedArray% intrinsic sitting above every concrete kind,
// its ACCESSOR Symbol.toStringTag, and how a typed array answers to the generic
// interrogations — Array.isArray, JSON.stringify, spread and Object.assign.

const TA: any = Object.getPrototypeOf(Uint8Array);
console.log("intrinsic_name=" + TA.name);
console.log("is_function=" + (typeof TA === "function"));
console.log("shared_by_all=" + (Object.getPrototypeOf(Int16Array) === TA) + "," + (Object.getPrototypeOf(Float64Array) === TA) + "," + (Object.getPrototypeOf(BigInt64Array) === TA));
console.log("above_intrinsic=" + (Object.getPrototypeOf(TA) === Function.prototype));
console.log("proto_chain=" + (Object.getPrototypeOf(Uint8Array.prototype) === TA.prototype));
console.log("not_a_constructor=" + (function (): string {
  try {
    new TA(1);
    return "no-throw";
  } catch (e: any) {
    return e.constructor.name;
  }
})());
console.log("intrinsic_not_global=" + (typeof (globalThis as any).TypedArray));

// The tag is a getter on the intrinsic prototype, not an own data property.
const tagDesc = Object.getOwnPropertyDescriptor(TA.prototype, Symbol.toStringTag) as any;
console.log("tag_is_getter=" + (typeof tagDesc.get === "function") + " has_value=" + (tagDesc.value !== undefined));
console.log("tag_on_kind=" + (Object.getOwnPropertyDescriptor(Uint8Array.prototype, Symbol.toStringTag) === undefined));
console.log("tag_of_instance=" + Object.prototype.toString.call(new Uint32Array(1)));
console.log("tag_of_proto=" + Object.prototype.toString.call(Uint8Array.prototype));
console.log("tag_getter_on_plain=" + String(tagDesc.get.call({})));
console.log("tag_getter_on_typed=" + String(tagDesc.get.call(new Int8Array(1))));

// BYTES_PER_ELEMENT lives on both the constructor and the prototype.
console.log("bpe=" + Uint8Array.BYTES_PER_ELEMENT + "," + Int16Array.BYTES_PER_ELEMENT + "," + Float32Array.BYTES_PER_ELEMENT + "," + Float64Array.BYTES_PER_ELEMENT + "," + BigInt64Array.BYTES_PER_ELEMENT);
console.log("bpe_on_proto=" + (Uint8Array.prototype as any).BYTES_PER_ELEMENT + "," + (Float64Array.prototype as any).BYTES_PER_ELEMENT);
console.log("bpe_on_intrinsic=" + String(TA.BYTES_PER_ELEMENT));

// The generic interrogations.
const t = new Uint8Array([1, 2, 3]);
console.log("isArray=" + Array.isArray(t));
console.log("typeof=" + typeof t);
console.log("instanceof=" + (t instanceof Uint8Array) + "," + (t instanceof TA) + "," + (t instanceof Object));
console.log("json=" + JSON.stringify(t));
console.log("json_nested=" + JSON.stringify({ v: new Int16Array([-1, 2]) }));
console.log("json_float=" + JSON.stringify(new Float64Array([1, NaN, Infinity, -0])));
console.log("json_empty=" + JSON.stringify(new Uint8Array(0)));
console.log("assign=" + JSON.stringify(Object.assign({}, new Uint8Array([5, 6]))));
console.log("spread_array=" + [...t].join(","));
console.log("spread_object=" + JSON.stringify({ ...new Uint8Array([7]) }));
console.log("entries=" + JSON.stringify(Object.entries(new Uint8Array([8, 9]))));
console.log("values=" + Object.values(new Uint8Array([8, 9])).join(","));
console.log("string_coerce=" + String(new Uint8Array([1, 2])));
console.log("concat_coerce=" + ("" + new Int8Array([-1, 2])));
console.log("tostring_is_array_tostring=" + (Uint8Array.prototype.toString === Array.prototype.toString));
console.log("join_is_own=" + (Uint8Array.prototype.join === Array.prototype.join));

// from / of go through the concrete constructor.
console.log("from_iterable=" + Uint8Array.from(new Set([1, 2, 300])).join(","));
console.log("from_mapped=" + Uint8Array.from([1, 2], function (x) { return x * 3; }).join(","));
console.log("from_arraylike=" + Uint8Array.from({ length: 2, 0: 5, 1: 6 } as any).join(","));
console.log("of=" + Uint8Array.of(1, 300, -1).join(","));
console.log("from_string=" + Uint8Array.from("123" as any).join(","));
console.log("from_kind=" + Float32Array.from([1.5]).constructor.name);

// Constructor argument shapes and their RangeErrors.
const buf = new ArrayBuffer(8);
console.log("full=" + new Uint32Array(buf).length);
console.log("from_offset=" + new Uint32Array(buf, 4).length);
console.log("with_length=" + new Uint8Array(buf, 2, 3).length + " offset=" + new Uint8Array(buf, 2, 3).byteOffset);
const shapes: Array<[string, () => number]> = [
  ["misaligned_offset", function () { return new Uint32Array(buf, 2).length; }],
  ["offset_past_end", function () { return new Uint8Array(buf, 9).length; }],
  ["offset_at_end", function () { return new Uint8Array(buf, 8).length; }],
  ["length_past_end", function () { return new Uint8Array(buf, 0, 9).length; }],
  ["negative_offset", function () { return new Uint8Array(buf, -1).length; }],
  ["indivisible_buffer", function () { return new Uint32Array(new ArrayBuffer(6)).length; }],
  ["negative_length", function () { return new Uint8Array(-1).length; }],
  ["fractional_length", function () { return new Uint8Array(2.9).length; }],
];
for (const shape of shapes) {
  try {
    console.log("ctor_" + shape[0] + "=" + shape[1]());
  } catch (e: any) {
    console.log("ctor_" + shape[0] + "=" + e.constructor.name);
  }
}
console.log("ctor_from_typed=" + new Uint8Array(new Int32Array([1, 300])).join(","));
console.log("ctor_from_null=" + (function (): string {
  try {
    return String(new Uint8Array(null as any).length);
  } catch (e: any) {
    return e.constructor.name;
  }
})());
