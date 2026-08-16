// ONE thing: the six ways of "making an integer" side by side, and exactly
// where they stop agreeing. |0, ~~ and >>0 are ToInt32; >>>0 is ToUint32;
// Math.trunc and Math.floor are not 32-bit at all.

function row(x: number): void {
  console.log(
    String(x) +
      " | or0:" + (x | 0) +
      " | tilde:" + ~~x +
      " | shr0:" + (x >> 0) +
      " | ushr0:" + (x >>> 0) +
      " | trunc:" + Math.trunc(x) +
      " | floor:" + Math.floor(x)
  );
}

// --- small values: everything but floor agrees on negatives ---
row(3.7);
row(-3.7);
row(0.5);
row(-0.5);
row(-1.5);
row(0);
row(-0);

// --- the 32-bit family wraps here; trunc and floor do not ---
row(2147483647);
row(2147483648);
row(2147483649);
row(-2147483648);
row(-2147483649);
row(4294967295);
row(4294967296);
row(4294967297);
row(4294967297.9);
row(6442450944);
row(-4294967297);

// --- above 2^53 the double itself is already the limit ---
row(9007199254740992);
row(9007199254740993);
row(-9007199254740992);
row(1e21);
row(-1e21);

// --- non-finite: the 32-bit family answers 0, trunc keeps them ---
row(NaN);
row(Infinity);
row(-Infinity);
row(Number.MAX_VALUE);
row(Number.MIN_VALUE);

// --- only Math.trunc preserves negative zero ---
console.log("--- negative zero survival ---");
const producers: number[] = [-0.5, -0.9, -0, -0.0001];
for (const p of producers) {
  console.log(
    "in:" + String(p) +
      " trunc_isNeg0:" + Object.is(Math.trunc(p), -0) +
      " ceil_isNeg0:" + Object.is(Math.ceil(p), -0) +
      " or0_isNeg0:" + Object.is(p | 0, -0) +
      " tilde_isNeg0:" + Object.is(~~p, -0) +
      " round_isNeg0:" + Object.is(Math.round(p), -0)
  );
}

// --- the same conversions applied to non-numbers ---
console.log("--- coercion of the operand ---");
const alien: [string, any][] = [
  ["str3.7", "3.7"],
  ["strneg3.7", "-3.7"],
  ["strhex", "0x10"],
  ["strempty", ""],
  ["strpadded", "  12  "],
  ["null", null],
  ["undefined", undefined],
  ["true", true],
  ["emptyarr", []],
  ["arr7", [7]],
  ["obj", {}],
];
for (const pair of alien) {
  const a: any = pair[1];
  console.log(
    pair[0] +
      " | or0:" + (a | 0) +
      " | tilde:" + ~~a +
      " | ushr0:" + (a >>> 0) +
      " | trunc:" + Math.trunc(a) +
      " | Number:" + Number(a)
  );
}
