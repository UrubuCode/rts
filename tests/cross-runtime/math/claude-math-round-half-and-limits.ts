// ONE thing: Math.round is NOT floor(x + 0.5). The spec rounds half UP toward
// +Infinity, returns -0 for anything in [-0.5, -0), and has an explicit "if
// x < 0.5 return +0" clause that the floor(x+0.5) formula gets wrong at the
// largest double below 0.5.

function round(x: number): void {
  const r = Math.round(x);
  console.log(
    "round(" + String(x) + ") = " + String(r) +
      " neg0:" + Object.is(r, -0) +
      " | floor(x+0.5) = " + String(Math.floor(x + 0.5))
  );
}

// --- halves go up, which is asymmetric for negatives ---
round(0.5);
round(1.5);
round(2.5);
round(3.5);
round(-0.5);
round(-1.5);
round(-2.5);
round(-3.5);

// --- the interval that produces negative zero ---
round(-0);
round(-0.1);
round(-0.4);
round(-0.49999999999999994);
round(-0.5000000000000001);
round(0);
round(0.1);
round(0.4);

// --- the famous largest double below 0.5: the spec says +0, the formula says 1
round(0.49999999999999994);
round(0.5000000000000001);
round(1.4999999999999998);

// --- above 2^52 there is no fractional part left to round ---
round(4503599627370496);
round(4503599627370495.5);
round(4503599627370497);
round(9007199254740991);
round(9007199254740992);
round(-4503599627370496);
round(1e21);
round(-1e21);

// --- non-finite values pass through ---
round(NaN);
round(Infinity);
round(-Infinity);
round(Number.MAX_VALUE);
round(Number.MIN_VALUE);
round(-Number.MIN_VALUE);

// --- how round differs from its three neighbours on the same inputs ---
console.log("--- round vs floor vs ceil vs trunc ---");
const probes: number[] = [2.5, -2.5, 0.5, -0.5, -0.2, 2.4, -2.4, -0];
for (const p of probes) {
  console.log(
    String(p) +
      " round:" + String(Math.round(p)) +
      " floor:" + String(Math.floor(p)) +
      " ceil:" + String(Math.ceil(p)) +
      " trunc:" + String(Math.trunc(p)) +
      " round_neg0:" + Object.is(Math.round(p), -0) +
      " ceil_neg0:" + Object.is(Math.ceil(p), -0)
  );
}

// --- the argument is coerced by ToNumber first ---
console.log("--- coercion ---");
console.log("str=" + String(Math.round("2.5" as any)));
console.log("null=" + String(Math.round(null as any)));
console.log("null_neg0=" + Object.is(Math.round(null as any), -0));
console.log("undefined=" + String(Math.round(undefined as any)));
console.log("true=" + String(Math.round(true as any)));
console.log("emptyarr=" + String(Math.round([] as any)));
console.log("arr=" + String(Math.round([2.5] as any)));
console.log("valueof=" + String(Math.round({ valueOf: function () { return -2.5; } } as any)));
console.log("no_args=" + String((Math.round as any)()));
console.log("length=" + Math.round.length);
