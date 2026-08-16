// ONE thing: double arithmetic is commutative but NOT associative and NOT
// distributive — (a+b)+c and a+(b+c) are different doubles for ordinary values,
// so the ORDER of a summation changes its result. Every operation here is
// exactly specified, which is what makes the disagreement portable.

function pair(label: string, left: number, right: number): void {
  console.log(
    label +
      " | left:" + String(left) +
      " | right:" + String(right) +
      " | equal:" + (left === right) +
      " | diff:" + (left === right ? "0" : (left - right).toExponential(3))
  );
}

// --- addition is not associative ---
pair("assoc_add_1", (0.1 + 0.2) + 0.3, 0.1 + (0.2 + 0.3));
pair("assoc_add_2", (1e16 + 1) + 1, 1e16 + (1 + 1));
pair("assoc_add_3", (1 + 1e100) - 1e100, 1 + (1e100 - 1e100));
pair("assoc_add_4", (0.1 + 0.1) + 0.1, 0.1 + (0.1 + 0.1));
pair("assoc_add_5", (1e-320 + 1) - 1, 1e-320 + (1 - 1));

// --- multiplication is not associative either ---
pair("assoc_mul_1", (0.1 * 0.2) * 0.3, 0.1 * (0.2 * 0.3));
pair("assoc_mul_2", (1e308 * 10) * 1e-10, 1e308 * (10 * 1e-10));
pair("assoc_mul_3", (1e-320 * 1e10) * 1e10, 1e-320 * (1e10 * 1e10));
pair("assoc_mul_4", (3 * 0.1) * 10, 3 * (0.1 * 10));

// --- and multiplication does not distribute over addition ---
pair("distrib_1", 0.1 * (0.2 + 0.3), 0.1 * 0.2 + 0.1 * 0.3);
pair("distrib_2", 3 * (1 / 3), 3 / 3);
pair("distrib_3", (0.1 + 0.2) * 3, 0.1 * 3 + 0.2 * 3);

// --- but addition and multiplication ARE commutative, for every finite pair ---
const operands: number[] = [0.1, 0.2, 1e16, 1e-320, 3, -7.5, 1e308, Number.EPSILON];
const nonCommutative: string[] = [];
for (const a of operands) {
  for (const b of operands) {
    if (!Object.is(a + b, b + a)) nonCommutative.push("add:" + a + "," + b);
    if (!Object.is(a * b, b * a)) nonCommutative.push("mul:" + a + "," + b);
  }
}
console.log("non_commutative=[" + nonCommutative.join(" ") + "]");

// --- summation order changes the total: forwards, backwards, sorted ---
const terms: number[] = [1e16, 1, 1, 1, 1, 1, 1, 1, 1, 1, -1e16];
function sum(list: number[]): number {
  let acc = 0;
  for (const t of list) acc += t;
  return acc;
}
const forwards = sum(terms);
const backwards = sum(terms.slice().reverse());
const ascending = sum(terms.slice().sort((a, b) => Math.abs(a) - Math.abs(b)));
console.log("sum_forwards=" + String(forwards));
console.log("sum_backwards=" + String(backwards));
console.log("sum_by_magnitude=" + String(ascending));
console.log("orders_agree=" + (forwards === backwards) + "," + (forwards === ascending));
console.log("reduce_matches_loop=" + (terms.reduce((a, b) => a + b, 0) === forwards));

// --- Kahan compensation recovers what the naive loop lost ---
function kahan(list: number[]): number {
  let acc = 0;
  let comp = 0;
  for (const t of list) {
    const y = t - comp;
    const next = acc + y;
    comp = next - acc - y;
    acc = next;
  }
  return acc;
}
const tenths: number[] = [];
for (let i = 0; i < 10; i++) tenths.push(0.1);
console.log("naive_ten_tenths=" + String(sum(tenths)));
console.log("kahan_ten_tenths=" + String(kahan(tenths)));
console.log("naive_is_one=" + (sum(tenths) === 1) + " kahan_is_one=" + (kahan(tenths) === 1));
console.log("naive_terms=" + String(kahan(terms)) + " vs " + String(sum(terms)));

// --- a running product is order-dependent at the range edges ---
const factors: number[] = [1e300, 1e-300, 1e300, 1e-300];
function product(list: number[]): number {
  let acc = 1;
  for (const f of list) acc = acc * f;
  return acc;
}
console.log("product_forwards=" + String(product(factors)));
console.log("product_reordered=" + String(product([1e300, 1e300, 1e-300, 1e-300])));
console.log("product_overflows_first=" + String(1e300 * 1e300 * 1e-300 * 1e-300));

// --- subtraction is not the inverse of addition once a value is absorbed ---
console.log("absorb=" + ((1e16 + 1) - 1e16));
console.log("absorb_reverse=" + (1e16 - 1e16 + 1));
console.log("cancellation=" + String((1 + 1e-16) - 1));
console.log("catastrophic=" + String((1.0000001 - 1) * 1e7));
console.log("x_minus_x_is_pos_zero=" + Object.is(0.1 - 0.1, 0) + " neg=" + Object.is(-0.1 + 0.1, 0));

// --- division and the reciprocal do not agree ---
const recipDisagree: string[] = [];
for (const a of [1, 3, 7, 0.1, 49, 1e300]) {
  if (10 / a !== 10 * (1 / a)) recipDisagree.push(String(a));
}
console.log("div_vs_reciprocal_disagree=[" + recipDisagree.join(",") + "]");
console.log("sqrt_squared_disagree=" + (Math.sqrt(2) * Math.sqrt(2) === 2));
console.log("x_over_x=" + (0.1 / 0.1) + "," + (1e-320 / 1e-320) + "," + String(0 / 0));
