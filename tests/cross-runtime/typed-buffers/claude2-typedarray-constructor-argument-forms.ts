// Cross-runtime: the four constructor forms of a typed array — a length, an
// object (array-like or iterable), another typed array, and a buffer window —
// including which arguments coerce quietly and which raise a RangeError.

const t = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return "throw:" + e.constructor.name;
  }
};

// Length form: ToIndex, so a fractional or negative length is refused but a
// string, a boolean and null are converted.
console.log("len_plain=" + new Uint8Array(3).length);
console.log("len_string=" + t(function () { return new Uint8Array("3" as any).length; }));
console.log("len_fraction=" + t(function () { return new Uint8Array(2.5).length; }));
console.log("len_negative=" + t(function () { return new Uint8Array(-1); }));
console.log("len_null=" + t(function () { return new Uint8Array(null as any).length; }));
console.log("len_undefined=" + t(function () { return new Uint8Array(undefined as any).length; }));
console.log("len_true=" + t(function () { return new Uint8Array(true as any).length; }));
console.log("len_nan=" + t(function () { return new Uint8Array(NaN).length; }));
console.log("len_infinity=" + t(function () { return new Uint8Array(Infinity); }));
console.log("len_bigint=" + t(function () { return new Uint8Array(3n as any).length; }));
console.log("len_zeroed=" + new Uint8Array(3).join(","));

// Object form: an array-like reads .length and the holes become 0; an iterable
// is drained through its iterator.
console.log("arraylike=" + t(function () { return new Uint8Array({ length: 3, 0: 1, 2: 5 } as any).join(","); }));
console.log("arraylike_no_length=" + t(function () { return new Uint8Array({ 0: 1 } as any).length; }));
console.log("holes=" + new Uint8Array([1, , 3] as any).join(","));
console.log("iterable_set=" + t(function () { return new Uint8Array(new Set([1, 2, 3]) as any).join(","); }));
console.log("iterable_string=" + t(function () { return new Uint8Array("12" as any).length; }));
console.log("iterable_custom=" + t(function () {
  const src: any = { length: 9 };
  src[Symbol.iterator] = function* () { yield 1; yield 300; };
  return new Uint8Array(src).join(",");
}));
console.log("iterator_not_callable=" + t(function () {
  const src: any = { length: 1, 0: 5 };
  src[Symbol.iterator] = 7;
  return new Uint8Array(src).join(",");
}));
console.log("from_kind=" + t(function () { return new Uint8Array(new Float64Array([1.7, -1, 300])).join(","); }));
console.log("widen_kind=" + t(function () { return new Float64Array(new Uint8Array([1, 2])).join(","); }));
console.log("copy_not_share=" + t(function () {
  const src = new Uint8Array([1, 2]);
  const dst = new Uint8Array(src);
  dst[0] = 9;
  return src[0] + "/" + (dst.buffer === src.buffer);
}));

// Buffer form: byteOffset must be a multiple of the element size, and an
// inferred length must divide evenly.
console.log("buffer_whole=" + new Uint32Array(new ArrayBuffer(8)).length);
console.log("buffer_offset=" + t(function () { return new Uint32Array(new ArrayBuffer(8), 4).length; }));
console.log("buffer_misaligned=" + t(function () { return new Uint32Array(new ArrayBuffer(8), 2); }));
console.log("buffer_bad_total=" + t(function () { return new Uint32Array(new ArrayBuffer(6)); }));
console.log("buffer_offset_past=" + t(function () { return new Uint8Array(new ArrayBuffer(4), 5); }));
console.log("buffer_len_past=" + t(function () { return new Uint8Array(new ArrayBuffer(4), 2, 3); }));
console.log("buffer_negative_offset=" + t(function () { return new Uint8Array(new ArrayBuffer(4), -1); }));
console.log("buffer_shares=" + t(function () {
  const buf = new ArrayBuffer(4);
  const a = new Uint8Array(buf);
  const b = new Uint8Array(buf);
  a[0] = 6;
  return b[0] + "/" + (a.buffer === b.buffer);
}));

// Called without new, and the static builders.
console.log("no_new=" + t(function () { return (Uint8Array as any)(3); }));
console.log("from_mapped=" + Uint8Array.from([1, 2, 3], function (x) { return x * 2; }).join(","));
console.log("from_string=" + Uint8Array.from("123" as any).join(","));
console.log("from_index_arg=" + Uint8Array.from([9, 9], function (_v, i) { return i; }).join(","));
console.log("from_this=" + t(function () { return (Uint8Array.from.call(Int16Array, [1, 2]) as any).constructor.name; }));
console.log("of=" + Uint8Array.of(1, 300, -1).join(","));
console.log("of_empty=" + Uint8Array.of().length + " from_empty=" + Uint8Array.from([]).length);
console.log("ctor_length=" + Uint8Array.length + " from_length=" + Uint8Array.from.length + " of_length=" + Uint8Array.of.length);
