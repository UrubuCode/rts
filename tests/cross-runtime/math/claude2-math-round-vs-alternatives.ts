// ONE thing: Math.round is NOT floor(x + 0.5), and it is not toFixed(0) either.
// The three disagree at the ties, at negative halves, and at the double whose
// value is just under a half — 0.49999999999999994, where adding 0.5 rounds up
// in binary before the floor ever runs.

function row(x: number): void {
  const r = Math.round(x);
  console.log(
    String(x) +
      " | round:" + String(r) + (Object.is(r, -0) ? "(-0)" : "") +
      " | floor+0.5:" + String(Math.floor(x + 0.5)) +
      " | ceil-0.5:" + String(Math.ceil(x - 0.5)) +
      " | trunc:" + String(Math.trunc(x)) +
      " | floor:" + String(Math.floor(x)) +
      " | ceil:" + String(Math.ceil(x))
  );
}

// --- ordinary ties: round goes toward +Infinity, never away from zero ---
row(0.5);
row(1.5);
row(2.5);
row(3.5);
row(-0.5);
row(-1.5);
row(-2.5);
row(-3.5);

// --- the famous double just below a half ---
row(0.49999999999999994);
row(-0.49999999999999994);
row(4503599627370497.5);

// --- either side of a tie ---
row(0.4);
row(0.6);
row(-0.4);
row(-0.6);
row(2.49999);
row(2.50001);

// --- the zeros, which round and trunc keep and floor(x+0.5) destroys ---
row(0);
row(-0);
row(-0.2);

// --- above 2^52 every double is an integer, so all six agree ---
row(4503599627370496);
row(9007199254740992);
row(9007199254740993);
row(-9007199254740992);

// --- non-finite ---
row(Infinity);
row(-Infinity);
row(NaN);

// --- toFixed(0) is a DECIMAL rounding: half away from zero, and a string ---
const fixed: number[] = [0.5, 1.5, 2.5, -0.5, -1.5, -2.5, 0.49999999999999994, 1.005, -0];
for (const x of fixed) {
  console.log(
    "toFixed0(" + String(x) + ")=" + x.toFixed(0) +
      " round=" + String(Math.round(x)) +
      " agree=" + (x.toFixed(0) === String(Math.round(x)))
  );
}

// --- Math.round preserves -0 only through the (-0.5, -0] window ---
console.log("round(-0)_is_neg0=" + Object.is(Math.round(-0), -0));
console.log("round(-0.2)_is_neg0=" + Object.is(Math.round(-0.2), -0));
console.log("round(-0.5)_is_neg0=" + Object.is(Math.round(-0.5), -0));
console.log("round(-0.500001)_is_neg1=" + (Math.round(-0.500001) === -1));
console.log("round(0.5)_is_1=" + (Math.round(0.5) === 1));

// --- round coerces its argument first ---
console.log("round_string=" + String(Math.round("2.5" as any)));
console.log("round_bool=" + String(Math.round(true as any)));
console.log("round_null=" + String(Math.round(null as any)) + " neg0:" + Object.is(Math.round(null as any), 0));
console.log("round_array=" + String(Math.round([] as any)));
console.log("round_undefined=" + String(Math.round(undefined as any)));
console.log("round_length=" + Math.round.length);
