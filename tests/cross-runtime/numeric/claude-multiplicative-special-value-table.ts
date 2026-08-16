// ONE thing: the IEEE special-value tables for * and /, printed with the sign
// of every zero made visible. String() hides -0, so a table read through
// String() alone cannot tell 0*-1 from 0*1.

function mul(a: number, b: number): void {
  const r = a * b;
  console.log("mul " + String(a) + " * " + String(b) + " = " + String(r) + " neg0:" + Object.is(r, -0));
}

function div(a: number, b: number): void {
  const r = a / b;
  console.log("div " + String(a) + " / " + String(b) + " = " + String(r) + " neg0:" + Object.is(r, -0));
}

// --- multiplication: the sign is the XOR of the operand signs ---
mul(0, 0);
mul(0, -0);
mul(-0, 0);
mul(-0, -0);
mul(1, 0);
mul(-1, 0);
mul(1, -0);
mul(-1, -0);
mul(2, 3);
mul(-2, 3);
mul(-2, -3);

// --- infinity times zero is the canonical NaN site ---
mul(Infinity, 0);
mul(Infinity, -0);
mul(-Infinity, 0);
mul(-Infinity, -0);
mul(Infinity, 2);
mul(Infinity, -2);
mul(-Infinity, -2);
mul(Infinity, Infinity);
mul(Infinity, -Infinity);
mul(-Infinity, -Infinity);
mul(NaN, 0);
mul(NaN, Infinity);
mul(NaN, NaN);

// --- overflow and underflow at the ends of the range ---
mul(Number.MAX_VALUE, 2);
mul(Number.MAX_VALUE, -2);
mul(Number.MIN_VALUE, 0.5);
mul(Number.MIN_VALUE, -0.5);
mul(1e200, 1e200);
mul(1e-200, 1e-200);

// --- division: zero denominators are infinities, not errors ---
div(1, 0);
div(-1, 0);
div(1, -0);
div(-1, -0);
div(0, 0);
div(-0, 0);
div(0, -0);
div(-0, -0);

// --- an infinite denominator gives a signed zero ---
div(1, Infinity);
div(-1, Infinity);
div(1, -Infinity);
div(-1, -Infinity);
div(0, Infinity);
div(-0, Infinity);
div(0, -Infinity);

// --- an infinite numerator, and infinity over infinity ---
div(Infinity, 2);
div(Infinity, -2);
div(-Infinity, 2);
div(Infinity, 0);
div(Infinity, -0);
div(-Infinity, -0);
div(Infinity, Infinity);
div(-Infinity, Infinity);
div(NaN, 1);
div(1, NaN);

// --- ordinary quotients, exact and inexact ---
div(6, 3);
div(-6, 3);
div(1, 3);
div(1, 2);
div(Number.MIN_VALUE, 2);
div(Number.MAX_VALUE, 0.5);

// --- addition and subtraction close the zero table ---
console.log("--- additive zeros ---");
console.log("0+0_neg0=" + Object.is(0 + 0, -0));
console.log("0+neg0_neg0=" + Object.is(0 + -0, -0));
console.log("neg0+neg0_neg0=" + Object.is(-0 + -0, -0));
console.log("0-0_neg0=" + Object.is(0 - 0, -0));
console.log("neg0-0_neg0=" + Object.is(-0 - 0, -0));
console.log("0-neg0_neg0=" + Object.is(0 - -0, -0));
console.log("neg0-neg0_neg0=" + Object.is(-0 - -0, -0));
console.log("inf_minus_inf=" + String(Infinity - Infinity));
console.log("neginf_plus_inf=" + String(-Infinity + Infinity));
console.log("inf_plus_inf=" + String(Infinity + Infinity));
console.log("unary_minus_zero_neg0=" + Object.is(-0, -0));
console.log("unary_minus_of_zero_var=" + Object.is(-(0), -0));
