// ONE thing: the IEEE-754 bit pattern behind a Number, read through a DataView
// with an EXPLICIT endianness (so the host's byte order cannot leak into the
// output). Sign, exponent and mantissa are pinned for every value here, and a
// stored NaN comes back as the one canonical quiet NaN.

const buf = new ArrayBuffer(8);
const view = new DataView(buf);

function bits(x: number): string {
  view.setFloat64(0, x, false);
  let out = "";
  for (let i = 0; i < 8; i++) {
    const byte = view.getUint8(i);
    out += byte.toString(16).padStart(2, "0");
  }
  return out;
}

function parts(x: number): string {
  view.setFloat64(0, x, false);
  const hi = view.getUint32(0, false);
  const lo = view.getUint32(4, false);
  const sign = hi >>> 31;
  const exponent = (hi >>> 20) & 0x7ff;
  const mantissaHi = hi & 0xfffff;
  return "s" + sign + " e" + exponent + " m" + mantissaHi.toString(16) + ":" + lo.toString(16).padStart(8, "0");
}

const values: [string, number][] = [
  ["+0", 0],
  ["-0", -0],
  ["1", 1],
  ["-1", -1],
  ["2", 2],
  ["0.5", 0.5],
  ["0.1", 0.1],
  ["0.2", 0.2],
  ["0.3", 0.3],
  ["0.1+0.2", 0.1 + 0.2],
  ["1/3", 1 / 3],
  ["PI", Math.PI],
  ["E", Math.E],
  ["EPSILON", Number.EPSILON],
  ["MIN_VALUE", Number.MIN_VALUE],
  ["MAX_VALUE", Number.MAX_VALUE],
  ["MAX_SAFE_INTEGER", Number.MAX_SAFE_INTEGER],
  ["2^53", 2 ** 53],
  ["2^-1074", 2 ** -1074],
  ["smallest_normal", 2 ** -1022],
  ["largest_subnormal", 2 ** -1022 - 2 ** -1074],
  ["+Infinity", Infinity],
  ["-Infinity", -Infinity],
  ["NaN", NaN],
];

for (const pair of values) {
  console.log(pair[0] + " | " + bits(pair[1]) + " | " + parts(pair[1]));
}

// --- the round trip is exact for every one of them ---
const broken: string[] = [];
for (const pair of values) {
  view.setFloat64(0, pair[1], true);
  const back = view.getFloat64(0, true);
  const ok = Number.isNaN(pair[1]) ? Number.isNaN(back) : Object.is(back, pair[1]);
  if (!ok) broken.push(pair[0]);
}
console.log("roundtrip_failures=[" + broken.join(",") + "]");

// --- the two endiannesses are byte reversals of each other ---
view.setFloat64(0, 0.1, false);
const big: number[] = [];
for (let i = 0; i < 8; i++) big.push(view.getUint8(i));
view.setFloat64(0, 0.1, true);
const little: number[] = [];
for (let i = 0; i < 8; i++) little.push(view.getUint8(i));
console.log("big_endian=" + big.join(","));
console.log("little_endian=" + little.join(","));
console.log("is_reversal=" + (big.join(",") === little.slice().reverse().join(",")));
console.log("default_is_big_endian=" + (() => {
  view.setFloat64(0, 0.1);
  const d: number[] = [];
  for (let i = 0; i < 8; i++) d.push(view.getUint8(i));
  return d.join(",") === big.join(",");
})());

// --- a NaN keeps the canonical quiet PAYLOAD, but its sign bit is genuinely
//     implementation-defined: Infinity - Infinity comes back positive under
//     JavaScriptCore and negative under V8, measured. So only the low 63 bits
//     are asserted, and the sign is reported as "varies" rather than printed.
//     Math.log(-1) is absent for the same reason one level deeper: V8 answers a
//     NaN whose PAYLOAD is 7ff0000000000001 while JavaScriptCore answers the
//     canonical 7ff8000000000000, so not even the payload is portable once a
//     library function produced the NaN.
function payload(x: number): string {
  view.setFloat64(0, x, false);
  const hi = view.getUint32(0, false) & 0x7fffffff;
  return hi.toString(16).padStart(8, "0") + view.getUint32(4, false).toString(16).padStart(8, "0");
}
const nans: [string, number][] = [
  ["literal", NaN],
  ["0/0", 0 / 0],
  ["sqrt(-1)", Math.sqrt(-1)],
  ["Inf*0", Infinity * 0],
  ["Inf-Inf", Infinity - Infinity],
  ["parse", Number("x")],
  ["Inf%1", Infinity % 1],
];
const payloads: string[] = [];
for (const n of nans) {
  payloads.push(n[0] + ":" + payload(n[1]));
}
console.log("nan_payloads=" + payloads.join(" "));
const canonical = payload(NaN);
const oddOnes: string[] = [];
for (const n of nans) {
  if (payload(n[1]) !== canonical) oddOnes.push(n[0]);
}
console.log("non_canonical_payloads=[" + oddOnes.join(",") + "]");
console.log("all_are_nan=" + nans.every((n) => Number.isNaN(n[1])));

// --- the exponent field, read as a number, is biased by 1023 ---
const exps: string[] = [];
for (let k = -4; k <= 4; k++) {
  view.setFloat64(0, 2 ** k, false);
  exps.push(String(k) + ":" + String((view.getUint32(0, false) >>> 20) & 0x7ff));
}
console.log("biased_exponents=" + exps.join(" "));

// --- a 32-bit view of the same buffer, and the float32 truncation ---
const f32 = new Float32Array(1);
const u32 = new Uint32Array(f32.buffer);
const f32bits: string[] = [];
for (const v of [0, -0, 1, 0.1, Infinity, NaN, 2 ** -149]) {
  f32[0] = v;
  f32bits.push(String(v) + ":" + u32[0].toString(16).padStart(8, "0"));
}
console.log("float32_bits=" + f32bits.join(" "));

// --- and the integer views agree with the arithmetic ---
view.setFloat64(0, 1, false);
console.log("one_as_uint32_pair=" + view.getUint32(0, false) + "," + view.getUint32(4, false));
view.setUint32(0, 0x7ff00000, false);
view.setUint32(4, 0, false);
console.log("assembled_infinity=" + String(view.getFloat64(0, false)));
view.setUint32(0, 0x80000000, false);
view.setUint32(4, 0, false);
console.log("assembled_neg_zero=" + Object.is(view.getFloat64(0, false), -0));
view.setUint32(0, 0, false);
view.setUint32(4, 1, false);
console.log("assembled_min_value=" + (view.getFloat64(0, false) === Number.MIN_VALUE));
