// Cross-runtime: the BigInt element kinds. A store goes through ToBigInt — so a
// boolean and a numeric string convert while a Number does not — the width wraps
// modulo 2^64, and every operation refuses to mix a bigint kind with a number one.

const i64 = new BigInt64Array(3);
i64[0] = 2n ** 63n;
i64[1] = -(2n ** 63n) - 1n;
i64[2] = 2n ** 64n + 5n;
console.log("i64_wrap=" + Array.from(i64).join(","));

const u64 = new BigUint64Array(3);
u64[0] = -1n;
u64[1] = 2n ** 64n;
u64[2] = -(2n ** 63n);
console.log("u64_wrap=" + Array.from(u64).join(","));

// The wrap is exactly BigInt.asIntN / asUintN over the same width.
console.log("as_intn=" + (BigInt.asIntN(64, 2n ** 63n) === i64[0]));
console.log("as_uintn=" + (BigInt.asUintN(64, -1n) === u64[0]));
console.log("bpe=" + BigInt64Array.BYTES_PER_ELEMENT + "," + BigUint64Array.BYTES_PER_ELEMENT + "," + new BigInt64Array(0).BYTES_PER_ELEMENT);

const stored = function (v: any): string {
  try {
    const a = new BigInt64Array(1);
    a[0] = v;
    return String(a[0]);
  } catch (e: any) {
    return e.constructor.name;
  }
};

// ToBigInt, not ToNumber: true and "5" convert, null/undefined/a Number do not.
console.log("store_true=" + stored(true));
console.log("store_false=" + stored(false));
console.log("store_string=" + stored("5"));
console.log("store_empty_string=" + stored(""));
console.log("store_bad_string=" + stored("x"));
console.log("store_null=" + stored(null));
console.log("store_undefined=" + stored(undefined));
console.log("store_number=" + stored(1));
console.log("store_valueof_bigint=" + stored({ valueOf: function () { return 9n; } }));
console.log("store_valueof_number=" + stored({ valueOf: function () { return 9; } }));

const attempt = function (f: () => any): string {
  try {
    return String(f());
  } catch (e: any) {
    return e.constructor.name;
  }
};

// The constructor and from/of follow the same rule.
console.log("from_bigints=" + attempt(function () { return BigInt64Array.from([1n, -2n]).join(","); }));
console.log("from_numbers=" + attempt(function () { return BigInt64Array.from([1, 2] as any).join(","); }));
console.log("from_mapped=" + attempt(function () { return BigInt64Array.from([1, 2] as any, function (x: any) { return BigInt(x); }).join(","); }));
console.log("of=" + attempt(function () { return BigUint64Array.of(1n, 2n).join(","); }));
console.log("ctor_from_number_kind=" + attempt(function () { return new BigInt64Array(new Uint8Array([1, 2]) as any).join(","); }));
console.log("ctor_length=" + new BigInt64Array(2).length + " byteLength=" + new BigInt64Array(2).byteLength);

// set() accepts the other bigint kind and refuses a number kind.
console.log("set_bigint_source=" + attempt(function () { const a = new BigInt64Array(2); a.set(new BigUint64Array([7n, 8n]) as any); return a.join(","); }));
console.log("set_number_source=" + attempt(function () { const a = new BigInt64Array(2); a.set(new Uint8Array([1, 2]) as any); return a.join(","); }));
console.log("set_plain_bigints=" + attempt(function () { const a = new BigUint64Array(2); a.set([1n, 2n] as any); return a.join(","); }));

// Ordinary methods work, and comparisons keep bigint semantics.
console.log("sort=" + new BigInt64Array([3n, -1n, 2n]).sort().join(","));
console.log("fill=" + attempt(function () { return new BigInt64Array(3).fill(4n).join(","); }));
console.log("fill_number=" + attempt(function () { return (new BigInt64Array(3) as any).fill(4).join(","); }));
console.log("search=" + (function (): string {
  const a = new BigInt64Array([1n, 2n]);
  return a.indexOf(2n) + "/" + a.includes(2 as any) + "/" + a.indexOf(1 as any);
})());
console.log("loose_eq=" + (i64[2] == 5 as any) + " strict=" + (i64[2] === 5n));
console.log("join=" + new BigInt64Array([1n, 2n]).join("-") + " string=" + String(new BigUint64Array([3n])));
console.log("tag=" + Object.prototype.toString.call(new BigInt64Array(1)) + "," + Object.prototype.toString.call(new BigUint64Array(1)));
console.log("json=" + attempt(function () { return JSON.stringify(new BigInt64Array([1n])); }));

// DataView reaches the same width through its own pair of accessors.
console.log("dv_roundtrip=" + attempt(function () {
  const dv = new DataView(new ArrayBuffer(8));
  dv.setBigUint64(0, -1n as any);
  return dv.getBigUint64(0) + "/" + dv.getBigInt64(0);
}));
console.log("dv_number_arg=" + attempt(function () { return new DataView(new ArrayBuffer(8)).setBigUint64(0, 1 as any); }));
console.log("dv_endian=" + attempt(function () {
  const dv = new DataView(new ArrayBuffer(8));
  dv.setBigInt64(0, 1n, true);
  return dv.getBigInt64(0, true) + "/" + dv.getBigInt64(0, false);
}));
