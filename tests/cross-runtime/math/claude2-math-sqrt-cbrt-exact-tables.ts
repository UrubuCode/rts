// ONE thing: sqrt and cbrt where the answer is an EXACT double — perfect
// squares and perfect cubes, powers of two, and the signed zeros. The digits of
// an irrational root are implementation-approximated and are absent here; what
// is pinned is that a perfect root comes back as the integer itself.

// --- every perfect square below 1e4 round-trips exactly ---
const badSquares: string[] = [];
for (let i = 0; i <= 100; i++) {
  if (Math.sqrt(i * i) !== i) badSquares.push(String(i));
}
console.log("square_roundtrip_failures=[" + badSquares.join(",") + "]");

// --- and so does every power of two the exponent range allows ---
const badPow2: string[] = [];
for (let k = 0; k <= 500; k += 2) {
  const v = Math.pow(2, k);
  if (Math.sqrt(v) !== Math.pow(2, k / 2)) badPow2.push(String(k));
}
console.log("pow2_sqrt_failures=[" + badPow2.join(",") + "]");

// --- a hand table, printed as values ---
const squares: number[] = [0, 1, 4, 9, 16, 25, 100, 144, 1024, 65536, 16777216, 4294967296, 9007199254740992];
const sq: string[] = [];
for (const v of squares) {
  sq.push(String(v) + "->" + String(Math.sqrt(v)));
}
console.log("sqrt=" + sq.join(" "));

// --- Number.isInteger(sqrt) as a perfect-square test over a small range ---
const perfect: string[] = [];
for (let i = 0; i <= 50; i++) {
  if (Number.isInteger(Math.sqrt(i))) perfect.push(String(i));
}
console.log("perfect_squares_0_50=" + perfect.join(","));

// --- the signed zeros and the domain edge ---
console.log("sqrt(+0)=" + String(Math.sqrt(0)) + " neg0:" + Object.is(Math.sqrt(0), -0));
console.log("sqrt(-0)=" + String(Math.sqrt(-0)) + " neg0:" + Object.is(Math.sqrt(-0), -0));
console.log("sqrt(-1)=" + String(Math.sqrt(-1)));
console.log("sqrt(-1e-320)=" + String(Math.sqrt(-1e-320)));
console.log("sqrt(Infinity)=" + String(Math.sqrt(Infinity)));
console.log("sqrt(-Infinity)=" + String(Math.sqrt(-Infinity)));
console.log("sqrt(NaN)=" + String(Math.sqrt(NaN)));

// --- sqrt of a subnormal is a normal number, and sqrt of MIN_VALUE is exact
//     enough to square back into the same neighbourhood ---
console.log("sqrt(MIN_VALUE)=" + String(Math.sqrt(Number.MIN_VALUE)));
console.log("sqrt(MIN_VALUE)_squared_is_min=" + (Math.sqrt(Number.MIN_VALUE) * Math.sqrt(Number.MIN_VALUE) === Number.MIN_VALUE));
console.log("sqrt(MAX_VALUE)_is_finite=" + Number.isFinite(Math.sqrt(Number.MAX_VALUE)));

// --- cbrt: every perfect cube, both signs ---
const badCubes: string[] = [];
for (let i = -60; i <= 60; i++) {
  if (Math.cbrt(i * i * i) !== i) badCubes.push(String(i));
}
console.log("cube_roundtrip_failures=[" + badCubes.join(",") + "]");

const cubes: number[] = [0, 1, 8, 27, 64, 125, 1000, 1000000, 1073741824, -1, -8, -27, -1000];
const cb: string[] = [];
for (const v of cubes) {
  cb.push(String(v) + "->" + String(Math.cbrt(v)));
}
console.log("cbrt=" + cb.join(" "));

// --- cbrt accepts the whole real line, unlike sqrt ---
console.log("cbrt(+0)=" + String(Math.cbrt(0)) + " neg0:" + Object.is(Math.cbrt(0), -0));
console.log("cbrt(-0)=" + String(Math.cbrt(-0)) + " neg0:" + Object.is(Math.cbrt(-0), -0));
console.log("cbrt(Infinity)=" + String(Math.cbrt(Infinity)));
console.log("cbrt(-Infinity)=" + String(Math.cbrt(-Infinity)));
console.log("cbrt(NaN)=" + String(Math.cbrt(NaN)));
console.log("cbrt_is_odd=" + (Math.cbrt(-125) === -Math.cbrt(125)));

// --- (-8) ** (1/3) is NaN while cbrt(-8) is -2: the operator has no branch ---
console.log("pow_neg8_third=" + String(Math.pow(-8, 1 / 3)));
console.log("operator_neg8_third=" + String((-8) ** (1 / 3)));
console.log("cbrt_neg8=" + String(Math.cbrt(-8)));

// --- coercion, arity ---
console.log("sqrt_string=" + String(Math.sqrt("81" as any)));
console.log("sqrt_bool=" + String(Math.sqrt(true as any)));
console.log("cbrt_string=" + String(Math.cbrt("-27" as any)));
console.log("sqrt_no_args=" + String((Math.sqrt as any)()));
console.log("arity=" + Math.sqrt.length + "," + Math.cbrt.length);
