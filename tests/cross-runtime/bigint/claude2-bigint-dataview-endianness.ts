// ONE thing: the 64-bit binary views. setBigInt64/setBigUint64 write the same
// eight bytes and differ only in how they are READ back, the endianness flag
// reverses the byte order exactly, and every one of these entry points refuses
// a Number argument with a TypeError instead of coercing it.

const buf = new ArrayBuffer(16);
const view = new DataView(buf);
const bytes = new Uint8Array(buf);

function hex(offset: number): string {
  let out = "";
  for (let i = offset; i < offset + 8; i++) {
    out += bytes[i].toString(16).padStart(2, "0");
  }
  return out;
}

function write(label: string, v: bigint): void {
  view.setBigInt64(0, v, false);
  const be = hex(0);
  view.setBigInt64(0, v, true);
  const le = hex(0);
  view.setBigInt64(0, v, false);
  console.log(
    label + " | be:" + be + " | le:" + le +
      " | signed:" + String(view.getBigInt64(0, false)) +
      " | unsigned:" + String(view.getBigUint64(0, false))
  );
}

// --- the sign is only a reading convention over the same bytes ---
write("0n", 0n);
write("1n", 1n);
write("-1n", -1n);
write("255n", 255n);
write("-255n", -255n);
write("2^32", 2n ** 32n);
write("2^63-1", 2n ** 63n - 1n);
write("-2^63", -(2n ** 63n));
write("max_safe", BigInt(Number.MAX_SAFE_INTEGER));
write("0x0123456789abcdef", 0x0123456789abcdefn);

// --- the two endiannesses are byte reversals of each other ---
view.setBigUint64(0, 0x0123456789abcdefn, false);
const beBytes: number[] = [];
for (let i = 0; i < 8; i++) beBytes.push(bytes[i]);
view.setBigUint64(0, 0x0123456789abcdefn, true);
const leBytes: number[] = [];
for (let i = 0; i < 8; i++) leBytes.push(bytes[i]);
console.log("be_bytes=" + beBytes.join(","));
console.log("le_bytes=" + leBytes.join(","));
console.log("is_reversal=" + (beBytes.join(",") === leBytes.slice().reverse().join(",")));
view.setBigUint64(0, 1n, false);
console.log("default_flag_is_big_endian=" + (hex(0) === "0000000000000001"));

// --- the unsigned reading of a negative value is its two's complement ---
view.setBigInt64(0, -1n, false);
console.log("neg_one_unsigned=" + String(view.getBigUint64(0, false)));
console.log("neg_one_is_2p64_minus_1=" + (view.getBigUint64(0, false) === 2n ** 64n - 1n));
view.setBigInt64(0, -(2n ** 63n), false);
console.log("min_i64_unsigned=" + String(view.getBigUint64(0, false)));
console.log("asUintN_agrees=" + (view.getBigUint64(0, false) === BigInt.asUintN(64, -(2n ** 63n))));
view.setBigUint64(0, 2n ** 64n - 1n, false);
console.log("max_u64_signed=" + String(view.getBigInt64(0, false)));
console.log("asIntN_agrees=" + (view.getBigInt64(0, false) === BigInt.asIntN(64, 2n ** 64n - 1n)));

// --- values outside the 64-bit range wrap, they do not throw ---
view.setBigInt64(0, 2n ** 64n + 5n, false);
console.log("wrap_above=" + String(view.getBigInt64(0, false)));
view.setBigUint64(0, -5n, false);
console.log("wrap_negative_unsigned=" + String(view.getBigUint64(0, false)));
view.setBigInt64(0, 2n ** 200n, false);
console.log("wrap_huge=" + String(view.getBigInt64(0, false)));

function attempt(label: string, fn: () => any): void {
  try {
    console.log(label + "=" + String(fn()));
  } catch (e) {
    console.log(label + "!" + (e as any).constructor.name);
  }
}

// --- a Number is refused: these are the entry points that never coerce ---
attempt("set_number", () => {
  view.setBigInt64(0, 1 as any, false);
  return "written";
});
attempt("set_numeric_string", () => {
  view.setBigInt64(0, "1" as any, false);
  return "written";
});
attempt("set_null", () => {
  view.setBigUint64(0, null as any, false);
  return "written";
});
attempt("set_boolean", () => {
  view.setBigUint64(0, true as any, false);
  return "written";
});
attempt("float64_rejects_bigint", () => {
  view.setFloat64(0, 1n as any, false);
  return "written";
});
attempt("uint32_rejects_bigint", () => {
  view.setUint32(0, 1n as any, false);
  return "written";
});

// --- offsets are checked against the buffer length ---
attempt("offset_8_ok", () => {
  view.setBigInt64(8, 42n, false);
  return String(view.getBigInt64(8, false));
});
attempt("offset_9_overruns", () => String(view.getBigInt64(9, false)));
attempt("offset_negative", () => String(view.getBigInt64(-1, false)));
attempt("offset_fractional", () => String(view.getBigInt64(0.5, false)));
attempt("offset_string", () => String(view.getBigInt64("0" as any, false)));

// --- and the typed arrays over the same buffer read the same bytes ---
const i64 = new BigInt64Array(buf);
const u64 = new BigUint64Array(buf);
view.setBigInt64(0, -2n, true);
view.setBigInt64(8, 7n, true);
console.log("i64_view=" + Array.from(i64).map((v) => String(v)).join(","));
console.log("u64_view=" + Array.from(u64).map((v) => String(v)).join(","));
console.log("byte_length=" + i64.byteLength + " length=" + i64.length + " BYTES_PER_ELEMENT=" + BigInt64Array.BYTES_PER_ELEMENT);
attempt("typed_array_number_assign", () => {
  (i64 as any)[0] = 5;
  return String(i64[0]);
});
attempt("typed_array_from_numbers", () => Array.from(BigInt64Array.from([1, 2] as any)).join(","));
attempt("typed_array_from_bigints", () => Array.from(BigInt64Array.from([1n, 2n])).map((v) => String(v)).join(","));
console.log("sorted=" + Array.from(BigInt64Array.from([3n, -1n, 2n]).sort()).map((v) => String(v)).join(","));
