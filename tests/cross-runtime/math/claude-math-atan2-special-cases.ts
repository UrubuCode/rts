// ONE thing: Math.atan2's special-case table. The rows that return a signed
// ZERO or NaN are pinned exactly by the spec; the rows that return a multiple
// of pi are only "implementation-approximated", so those are checked against
// Math.PI rather than printed as digits.

function exact(label: string, y: number, x: number): void {
  const r = Math.atan2(y, x);
  console.log(
    label + " atan2(" + String(y) + "," + String(x) + ")" +
      " = " + String(r) +
      " neg0:" + Object.is(r, -0) +
      " pos0:" + Object.is(r, 0)
  );
}

// --- either argument NaN gives NaN ---
exact("nan_y", NaN, 1);
exact("nan_x", 1, NaN);
exact("nan_both", NaN, NaN);
exact("nan_inf", NaN, Infinity);

// --- y is +0: the answer is +0 or pi depending on the sign of x ---
exact("p0_p0", 0, 0);
exact("p0_pos", 0, 1);
exact("p0_posinf", 0, Infinity);
exact("p0_n0", 0, -0);
exact("p0_neg", 0, -1);

// --- y is -0: the mirror image, and the zero it returns is NEGATIVE ---
exact("n0_p0", -0, 0);
exact("n0_pos", -0, 1);
exact("n0_posinf", -0, Infinity);
exact("n0_n0", -0, -0);
exact("n0_neg", -0, -1);

// --- a finite non-zero y against an infinite x collapses to a signed zero ---
exact("pos_posinf", 1, Infinity);
exact("neg_posinf", -1, Infinity);
exact("big_posinf", 1e300, Infinity);
exact("tiny_posinf", Number.MIN_VALUE, Infinity);

// --- a finite x against an infinite y is +-pi/2, checked below ---
exact("posinf_p0", Infinity, 0);
exact("neginf_p0", -Infinity, 0);

// --- the pi-family rows, as identities rather than digits ---
console.log("--- pi identities ---");
const HALF = Math.PI / 2;
console.log("y_pos_x_p0_is_half=" + (Math.atan2(1, 0) === HALF));
console.log("y_pos_x_n0_is_half=" + (Math.atan2(1, -0) === HALF));
console.log("y_neg_x_p0_is_neghalf=" + (Math.atan2(-1, 0) === -HALF));
console.log("y_neg_x_n0_is_neghalf=" + (Math.atan2(-1, -0) === -HALF));
console.log("y_p0_x_n0_is_pi=" + (Math.atan2(0, -0) === Math.PI));
console.log("y_p0_x_neg_is_pi=" + (Math.atan2(0, -1) === Math.PI));
console.log("y_n0_x_n0_is_negpi=" + (Math.atan2(-0, -0) === -Math.PI));
console.log("y_n0_x_neg_is_negpi=" + (Math.atan2(-0, -1) === -Math.PI));
console.log("y_pos_x_neginf_is_pi=" + (Math.atan2(1, -Infinity) === Math.PI));
console.log("y_neg_x_neginf_is_negpi=" + (Math.atan2(-1, -Infinity) === -Math.PI));
console.log("y_posinf_x_finite_is_half=" + (Math.atan2(Infinity, 5) === HALF));
console.log("y_neginf_x_finite_is_neghalf=" + (Math.atan2(-Infinity, 5) === -HALF));
console.log("y_posinf_x_neg_is_half=" + (Math.atan2(Infinity, -5) === HALF));
console.log("y_neginf_x_neg_is_neghalf=" + (Math.atan2(-Infinity, -5) === -HALF));

// --- symmetry that must hold whatever the last digit is ---
console.log("--- symmetry ---");
console.log("neg_y_flips=" + (Math.atan2(-3, 4) === -Math.atan2(3, 4)));
console.log("neg_y_flips2=" + (Math.atan2(-1, -1) === -Math.atan2(1, -1)));
console.log("quadrant1_positive=" + (Math.atan2(1, 1) > 0));
console.log("quadrant2_gt_half=" + (Math.atan2(1, -1) > HALF));
console.log("quadrant3_lt_neghalf=" + (Math.atan2(-1, -1) < -HALF));
console.log("quadrant4_negative=" + (Math.atan2(-1, 1) < 0));
console.log("in_range=" + (Math.atan2(1, -1) <= Math.PI && Math.atan2(-1, -1) >= -Math.PI));

// --- arity and argument coercion ---
console.log("--- arity and coercion ---");
console.log("length=" + Math.atan2.length);
console.log("no_args_is_nan=" + Number.isNaN((Math.atan2 as any)()));
console.log("one_arg_is_nan=" + Number.isNaN((Math.atan2 as any)(1)));
console.log("string_args=" + (Math.atan2("1" as any, "0" as any) === HALF));
console.log("null_y_x_neg_is_pi=" + (Math.atan2(null as any, -1) === Math.PI));
console.log("undefined_is_nan=" + Number.isNaN(Math.atan2(undefined as any, 1)));
const order: string[] = [];
const y: any = { valueOf: function () { order.push("y"); return 0; } };
const x: any = { valueOf: function () { order.push("x"); return 1; } };
console.log("coerced=" + String(Math.atan2(y, x)));
console.log("order=" + order.join(","));
