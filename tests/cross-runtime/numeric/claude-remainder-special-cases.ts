// ONE thing: the complete special-value table of `%`. The sign of the result
// follows the DIVIDEND, never the divisor, and that holds all the way down to
// negative zero — which no string rendering will show you.

function rem(a: number, b: number): void {
  const r = a % b;
  console.log(
    String(a) + " % " + String(b) +
      " = " + String(r) +
      " | isNeg0:" + Object.is(r, -0) +
      " | isNaN:" + Number.isNaN(r)
  );
}

// --- NaN poisons either side ---
rem(NaN, 1);
rem(1, NaN);
rem(NaN, NaN);
rem(NaN, Infinity);

// --- an infinite dividend is NaN whatever the divisor ---
rem(Infinity, 2);
rem(-Infinity, 2);
rem(Infinity, -2);
rem(Infinity, Infinity);
rem(-Infinity, Infinity);
rem(Infinity, 0);

// --- an infinite divisor returns the dividend unchanged, sign included ---
rem(2, Infinity);
rem(-2, Infinity);
rem(2, -Infinity);
rem(-2, -Infinity);
rem(0, Infinity);
rem(-0, Infinity);
rem(0.5, Infinity);

// --- a zero divisor is NaN; a zero dividend keeps its own sign ---
rem(5, 0);
rem(-5, 0);
rem(5, -0);
rem(0, 0);
rem(-0, -0);
rem(0, 5);
rem(-0, 5);
rem(-0, -5);
rem(0, -5);

// --- ordinary integers: four sign combinations ---
rem(7, 3);
rem(-7, 3);
rem(7, -3);
rem(-7, -3);
rem(6, 3);
rem(-6, 3);
rem(-6, -3);

// --- the dividend sign survives even when the result is zero ---
console.log("neg6_mod3_isNeg0=" + Object.is(-6 % 3, -0));
console.log("pos6_mod3_isNeg0=" + Object.is(6 % 3, -0));
console.log("neg6_mod_neg3_isNeg0=" + Object.is(-6 % -3, -0));

// --- fractions: % is the IEEE truncated remainder, not a modulo ---
rem(5.5, 2);
rem(-5.5, 2);
rem(5.5, -2);
rem(0.3, 0.1);
rem(1, 0.3);
rem(-1, 0.3);

// --- very large dividends stay exact where the double is exact ---
rem(9007199254740992, 2);
rem(9007199254740991, 2);
rem(1e21, 3);
rem(Number.MAX_VALUE, 2);
rem(Number.MIN_VALUE, Number.MIN_VALUE);

// --- the operands are coerced by ToNumber, in left-then-right order ---
const seen: string[] = [];
const lhs: any = { valueOf: function () { seen.push("lhs"); return 7; } };
const rhs: any = { valueOf: function () { seen.push("rhs"); return 3; } };
console.log("coerced=" + String(lhs % rhs));
console.log("coercion_order=" + seen.join(","));
console.log("str_mod=" + String(("7" as any) % ("3" as any)));
console.log("bool_mod=" + String((true as any) % (2 as any)));
console.log("null_mod=" + String((null as any) % (2 as any)));
console.log("undef_mod=" + String((undefined as any) % (2 as any)));
console.log("arr_mod=" + String(([7] as any) % ([3] as any)));

// --- the assignment form agrees with the operator ---
let acc = -7;
acc %= 3;
console.log("compound=" + String(acc));
