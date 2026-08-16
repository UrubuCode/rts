// ONE thing: the Number::exponentiate special-case table, exhaustively.
// These rows are pinned by the spec, unlike the digits of a general power.
// Note 1 ** NaN is NaN and (-1) ** Infinity is NaN — both changed in ES2016.

function pow(a: number, b: number): void {
  const viaMethod = Math.pow(a, b);
  const viaOperator = a ** b;
  console.log(
    String(a) + " ** " + String(b) +
      " = " + String(viaMethod) +
      " neg0:" + Object.is(viaMethod, -0) +
      " agree:" + (Object.is(viaMethod, viaOperator) || (Number.isNaN(viaMethod) && Number.isNaN(viaOperator)))
  );
}

// --- an exponent of zero wins over everything, NaN included ---
pow(NaN, 0);
pow(NaN, -0);
pow(Infinity, 0);
pow(-Infinity, -0);
pow(0, 0);
pow(-0, 0);
pow(0, -0);

// --- a NaN exponent otherwise poisons, and a base of 1 does NOT rescue it ---
pow(1, NaN);
pow(-1, NaN);
pow(2, NaN);
pow(NaN, NaN);
pow(NaN, 1);
pow(NaN, -1);

// --- |base| == 1 with an infinite exponent is NaN, not 1 ---
pow(1, Infinity);
pow(1, -Infinity);
pow(-1, Infinity);
pow(-1, -Infinity);

// --- |base| > 1 and |base| < 1 with infinite exponents ---
pow(2, Infinity);
pow(2, -Infinity);
pow(-2, Infinity);
pow(-2, -Infinity);
pow(0.5, Infinity);
pow(0.5, -Infinity);
pow(-0.5, Infinity);
pow(-0.5, -Infinity);
pow(0, Infinity);
pow(0, -Infinity);
pow(-0, Infinity);
pow(-0, -Infinity);

// --- an infinite base: the sign survives only for odd integer exponents ---
pow(Infinity, 1);
pow(Infinity, 2);
pow(Infinity, -1);
pow(Infinity, -2);
pow(-Infinity, 3);
pow(-Infinity, 2);
pow(-Infinity, 2.5);
pow(-Infinity, -3);
pow(-Infinity, -2);
pow(-Infinity, -2.5);

// --- a zero base, both signs, both directions ---
pow(0, 3);
pow(0, -3);
pow(0, 0.5);
pow(-0, 3);
pow(-0, 2);
pow(-0, 2.5);
pow(-0, -3);
pow(-0, -2);
pow(-0, -2.5);
pow(0, -0.5);

// --- a negative base with a non-integer exponent has no real answer ---
pow(-2, 0.5);
pow(-8, 1 / 3);
pow(-1, 0.5);
pow(-4, 1.5);
pow(-2, -0.5);

// --- exact integer powers that every engine must agree on ---
pow(2, 10);
pow(2, 53);
pow(-2, 3);
pow(-2, 4);
pow(10, 21);
pow(4, 0.5);
pow(9, 0.5);
pow(2, -2);

// --- the operator is right-associative and refuses an unparenthesised
//     unary minus on the left, so this is the only legal spelling ---
console.log("right_assoc=" + String(2 ** 3 ** 2));
console.log("paren_left=" + String((2 ** 3) ** 2));
console.log("neg_base=" + String((-2) ** 2));
console.log("compound=" + (function () { let x = 3; x **= 2; return String(x); })());

// --- the operands are coerced left to right, before any comparison ---
const order: string[] = [];
const base: any = { valueOf: function () { order.push("base"); return 2; } };
const exp: any = { valueOf: function () { order.push("exp"); return 3; } };
console.log("coerced=" + String(Math.pow(base, exp)));
console.log("order=" + order.join(","));
console.log("string_operands=" + String(Math.pow("2" as any, "3" as any)));
console.log("null_base=" + String(Math.pow(null as any, 2)));
console.log("undef_exp=" + String(Math.pow(2, undefined as any)));
