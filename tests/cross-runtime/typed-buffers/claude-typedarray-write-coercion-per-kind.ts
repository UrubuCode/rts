// Cross-runtime: how a value is coerced when it is WRITTEN into a typed array.
// Each element kind converts differently — wrapping, sign, clamping with
// round-half-to-even, truncation toward zero, and NaN becoming 0 for integers.

const u8 = new Uint8Array(6);
u8[0] = 256;
u8[1] = 257;
u8[2] = -1;
u8[3] = 1.9;
u8[4] = -1.9;
u8[5] = NaN;
console.log("u8=" + Array.from(u8).join(","));

const u16 = new Uint16Array(4);
u16[0] = 65536;
u16[1] = 65537;
u16[2] = -1;
u16[3] = 70000;
console.log("u16=" + Array.from(u16).join(","));

const i8 = new Int8Array(5);
i8[0] = 127;
i8[1] = 128;
i8[2] = 255;
i8[3] = -129;
i8[4] = 1e10;
console.log("i8=" + Array.from(i8).join(","));

const i32 = new Int32Array(4);
i32[0] = 2147483647;
i32[1] = 2147483648;
i32[2] = 4294967296;
i32[3] = -2147483649;
console.log("i32=" + Array.from(i32).join(","));

// Uint8ClampedArray clamps instead of wrapping, and rounds half to EVEN.
const c = new Uint8ClampedArray(9);
c[0] = -5;
c[1] = 300;
c[2] = 0.5;
c[3] = 1.5;
c[4] = 2.5;
c[5] = 3.5;
c[6] = -0.6;
c[7] = NaN;
c[8] = 254.5;
console.log("clamped=" + Array.from(c).join(","));

// Infinity clamps at the ends for clamped, becomes 0 for wrapping kinds.
const c2 = new Uint8ClampedArray(2);
c2[0] = Infinity;
c2[1] = -Infinity;
console.log("clamped_inf=" + Array.from(c2).join(","));
const u8i = new Uint8Array(2);
u8i[0] = Infinity;
u8i[1] = -Infinity;
console.log("u8_inf=" + Array.from(u8i).join(","));

// Floats keep the value they can represent, and -0 stays -0.
const f32 = new Float32Array(5);
f32[0] = 0.1;
f32[1] = -0;
f32[2] = 1e39;
f32[3] = 16777217;
f32[4] = NaN;
console.log("f32_0=" + f32[0]);
console.log("f32_negzero=" + Object.is(f32[1], -0));
console.log("f32_over=" + f32[2]);
console.log("f32_int=" + f32[3]);
console.log("f32_nan=" + Number.isNaN(f32[4]));

const f64 = new Float64Array(3);
f64[0] = 0.1;
f64[1] = -0;
f64[2] = 1e39;
console.log("f64_0=" + f64[0]);
console.log("f64_negzero=" + Object.is(f64[1], -0));
console.log("f64_over=" + f64[2]);

// -0 written into an integer kind reads back as +0.
const i16 = new Int16Array(1);
i16[0] = -0;
console.log("i16_negzero=" + Object.is(i16[0], -0) + "," + Object.is(i16[0], 0));

// Non-numbers go through ToNumber first.
const mix = new Uint8Array(6);
mix[0] = "12" as any;
mix[1] = true as any;
mix[2] = null as any;
mix[3] = undefined as any;
mix[4] = "" as any;
mix[5] = { valueOf: function () { return 7; } } as any;
console.log("mix=" + Array.from(mix).join(","));

// A rejected ToNumber throws instead of writing.
const bad = new Uint8Array(1);
bad[0] = 5;
try {
  bad[0] = { valueOf: function () { throw new RangeError("nope"); } } as any;
  console.log("throwing_valueof=no-throw");
} catch (e: any) {
  console.log("throwing_valueof=" + e.constructor.name);
}
console.log("bad_kept=" + bad[0]);

// Symbols cannot be converted at all.
try {
  (new Uint8Array(1) as any)[0] = Symbol("s");
  console.log("symbol=no-throw");
} catch (e: any) {
  console.log("symbol=" + e.constructor.name);
}

// BigInt kinds wrap by their own width and refuse a Number.
const b64 = new BigInt64Array(3);
b64[0] = 2n ** 63n;
b64[1] = -1n;
b64[2] = 2n ** 64n + 5n;
console.log("bigint64=" + b64[0] + "," + b64[1] + "," + b64[2]);
const bu64 = new BigUint64Array(2);
bu64[0] = -1n;
bu64[1] = 2n ** 64n;
console.log("biguint64=" + bu64[0] + "," + bu64[1]);
try {
  (b64 as any)[0] = 1;
  console.log("bigint_takes_number=no-throw");
} catch (e: any) {
  console.log("bigint_takes_number=" + e.constructor.name);
}
try {
  (new Uint8Array(1) as any)[0] = 1n;
  console.log("number_takes_bigint=no-throw");
} catch (e: any) {
  console.log("number_takes_bigint=" + e.constructor.name);
}
