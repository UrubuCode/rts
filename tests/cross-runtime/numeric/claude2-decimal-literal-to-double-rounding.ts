// ONE thing: a decimal literal is ROUNDED to the nearest double at parse time,
// ties to even — so distinct literals collide, and the value actually stored is
// not the one written. toFixed(20) is the microscope: it prints the exact
// decimal expansion of the stored double, not the digits of the source.

function exact(label: string, x: number): void {
  console.log(label + " | stored:" + String(x) + " | exact:" + x.toFixed(20));
}

// --- the classics: what 0.1 really is ---
exact("0.1", 0.1);
exact("0.2", 0.2);
exact("0.3", 0.3);
exact("0.1+0.2", 0.1 + 0.2);
exact("0.7", 0.7);
exact("1.005", 1.005);
exact("2.675", 2.675);
exact("8.575", 8.575);
exact("1.5", 1.5);
exact("0.25", 0.25);
exact("EPSILON", Number.EPSILON);

// --- the sum is a different double from the literal, by exactly one ulp ---
console.log("sum_vs_literal=" + (0.1 + 0.2 === 0.3));
console.log("sum_is_the_next_double=" + (0.1 + 0.2 === 0.30000000000000004));
console.log("gap=" + (0.1 + 0.2 - 0.3).toExponential(4));
console.log("rounded_equal=" + (Math.abs(0.1 + 0.2 - 0.3) < Number.EPSILON));

// --- distinct literals that land on the SAME double ---
const collisions: [string, number, number][] = [
  ["0.1", 0.1, 0.1000000000000000055511151231257827],
  ["1e-1", 1e-1, 0.1],
  ["one_third", 1 / 3, 0.3333333333333333],
  ["max_safe", 9007199254740993, 9007199254740992],
  ["max_safe_plus2", 9007199254740994, 9007199254740994],
  ["big_odd", 10000000000000001, 10000000000000000],
  ["huge", 1e23, 99999999999999991611392],
  ["huge_neighbour", 1.0000000000000001e23, 1e23],
  ["tiny", 5e-324, 3e-324],
  ["tiny_round_down", 2e-324, 0],
];
for (const c of collisions) {
  console.log("collide_" + c[0] + "=" + (c[1] === c[2]) + " value=" + String(c[1]));
}

// --- ties to even at the integer boundary: 2^53 counts by twos ---
const boundary: number[] = [
  9007199254740991, 9007199254740992, 9007199254740993, 9007199254740994,
  9007199254740995, 9007199254740996, 9007199254740997,
];
const asStored: string[] = [];
for (const b of boundary) {
  asStored.push(String(b));
}
console.log("above_2p53=" + asStored.join(" "));
console.log("odd_rounds_to_even=" + (9007199254740993 === 9007199254740992) + "," + (9007199254740995 === 9007199254740996));
console.log("18014398509481985=" + String(18014398509481985) + " (2^54+1, counts by fours)");

// --- 1e23 is famously not 10^23, and the exact digits say so ---
console.log("1e23_exact=" + (1e23).toFixed(0));
console.log("1e22_exact=" + (1e22).toFixed(0));
console.log("1e23_is_integer=" + Number.isInteger(1e23));
console.log("1e23_neighbours=" + (1e23 === 99999999999999991611392) + "," + (1e23 === 100000000000000000000000));

// --- the same rounding applies to Number(), not just to a literal ---
const parsed: string[] = [
  "0.1", "0.30000000000000004", "9007199254740993", "1e23",
  "0.1000000000000000055511151231257827", "1.7976931348623159e308",
  "4.9e-324", "2.4e-324", "2.5e-324",
];
for (const p of parsed) {
  console.log("Number(" + p + ")=" + String(Number(p)));
}
console.log("literal_matches_parse=" + (Number("0.1") === 0.1) + "," + (Number("9007199254740993") === 9007199254740993));

// --- and to the round trip: String(x) is the SHORTEST literal that reparses ---
const roundtrip: number[] = [
  0.1, 0.3, 1 / 3, 1e23, 5e-324, Number.MAX_VALUE, Number.EPSILON,
  0.1 + 0.2, 123456789.123456789, 2 ** 53 + 2, 1.7976931348623157e308,
];
const broken: string[] = [];
for (const r of roundtrip) {
  if (Number(String(r)) !== r) broken.push(String(r));
}
console.log("roundtrip_failures=[" + broken.join(",") + "]");
const shortest: string[] = [];
for (const r of roundtrip) {
  shortest.push(String(r) + ":" + String(String(r).length));
}
console.log("shortest_forms=" + shortest.join(" "));

// --- adding a digit that the double cannot hold changes nothing ---
console.log("extra_digits_ignored=" + (0.1 === 0.10000000000000000000001));
console.log("extra_digits_matter=" + (0.1 === 0.10000000000000002));
console.log("trailing_zeros=" + (1.5 === 1.50000000000000000000));
console.log("exponent_forms_agree=" + (1e2 === 100) + "," + (1.5e-3 === 0.0015));
