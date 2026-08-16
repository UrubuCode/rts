// ONE thing: Math.fround is the float32 rounding, and its boundaries are the
// float32 ones — largest finite, overflow to Infinity, smallest normal,
// smallest subnormal, and the underflow to a signed zero. It must agree with a
// Float32Array round-trip on every input, which is the definition.

function f32(x: number): number {
  const a = new Float32Array(1);
  a[0] = x;
  return a[0];
}

function row(label: string, x: number): void {
  const r = Math.fround(x);
  console.log(
    label + " = " + String(r) +
      (Object.is(r, -0) ? "(-0)" : "") +
      " | array_agrees:" + Object.is(r, f32(x)) +
      " | idempotent:" + Object.is(r, Math.fround(r))
  );
}

// --- the specials pass straight through ---
row("NaN", NaN);
row("+0", 0);
row("-0", -0);
row("+Inf", Infinity);
row("-Inf", -Infinity);

// --- exactly representable values are untouched ---
row("1", 1);
row("-1", -1);
row("0.5", 0.5);
row("0.25", 0.25);
row("1.5", 1.5);
row("16777216", 16777216);
row("16777217", 16777217);
row("16777218", 16777218);
row("2^127", 2 ** 127);
row("2^-126", 2 ** -126);

// --- the largest finite float32, and the first double above it ---
row("FLT_MAX", 3.4028234663852886e38);
row("FLT_MAX_plus_a_bit", 3.402823466385289e38);
row("overflow_midpoint", 3.4028235677973366e38);
row("just_below_overflow", 3.4028234663852886e38 * 1.0000001);
row("2^128", 2 ** 128);
row("1e39", 1e39);
row("-1e39", -1e39);
row("MAX_VALUE", Number.MAX_VALUE);
row("-MAX_VALUE", -Number.MAX_VALUE);

// --- the subnormal floor: half of the smallest subnormal underflows to zero
//     with its sign kept ---
row("FLT_MIN_normal", 1.1754943508222875e-38);
row("FLT_TRUE_MIN", 1.401298464324817e-45);
row("half_of_true_min", 1.401298464324817e-45 / 2);
row("neg_half_of_true_min", -1.401298464324817e-45 / 2);
row("1e-46", 1e-46);
row("-1e-46", -1e-46);
row("MIN_VALUE", Number.MIN_VALUE);
row("-MIN_VALUE", -Number.MIN_VALUE);

// --- ordinary decimals lose their tail ---
row("0.1", 0.1);
row("0.2", 0.2);
row("1.337", 1.337);
row("one_third", 1 / 3);
row("PI", Math.PI);
row("E", Math.E);
row("1.0000001", 1.0000001);

// --- fround is a projection: a second pass never moves the value ---
const probes: number[] = [0.1, 1 / 3, Math.PI, 1e30, 1e-30, 123456.789, -987654.321];
const notIdempotent: string[] = [];
for (const p of probes) {
  const once = Math.fround(p);
  if (!Object.is(once, Math.fround(once)) || !Object.is(once, Math.fround(Math.fround(once)))) {
    notIdempotent.push(String(p));
  }
}
console.log("not_idempotent=[" + notIdempotent.join(",") + "]");

// --- and it agrees with the typed array on a sweep ---
const mismatch: string[] = [];
for (let i = -40; i <= 40; i++) {
  const v = i / 7;
  if (!Object.is(Math.fround(v), f32(v))) mismatch.push(String(v));
}
console.log("sweep_mismatch=[" + mismatch.join(",") + "]");

// --- coercion and arity ---
console.log("fround_string=" + String(Math.fround("1.5" as any)));
console.log("fround_null=" + String(Math.fround(null as any)) + " neg0:" + Object.is(Math.fround(null as any), -0));
console.log("fround_undefined=" + String(Math.fround(undefined as any)));
console.log("fround_array=" + String(Math.fround([2.5] as any)));
console.log("fround_no_args=" + String((Math.fround as any)()));
console.log("fround_length=" + Math.fround.length);
console.log("fround_name=" + Math.fround.name);

// --- float32 precision is 24 bits: the gap opens at 2^24 ---
console.log("gap_at_2p24=" + (Math.fround(16777217) === 16777216));
console.log("gap_at_2p24_next=" + (Math.fround(16777219) === 16777220));
console.log("no_gap_below=" + (Math.fround(16777215) === 16777215));
