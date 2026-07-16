// Cross-runtime: numeric coercion of Date — valueOf/getTime/Number()/unary +
// must all agree, including on Invalid Date.

const d = new Date(Date.UTC(2024, 5, 10, 7, 8, 9, 10));

console.log("getTime=" + d.getTime());
console.log("valueOf=" + d.valueOf());
console.log("Number=" + Number(d));
console.log("unary_plus=" + +d);

// All four agree.
console.log("agree1=" + (d.getTime() === d.valueOf()));
console.log("agree2=" + (Number(d) === d.getTime()));
console.log("agree3=" + (+d === d.getTime()));

// Arithmetic forces ToNumber.
console.log("times_one=" + (d as any) * 1);
console.log("minus_zero=" + ((d as any) - 0));
console.log("div=" + (d as any) / 1000);

// Math.* forces ToNumber too.
console.log("math_floor=" + Math.floor((d as any) / 1000));
console.log("math_trunc=" + Math.trunc(Number(d) / 86400000));

// Epoch and negative instants.
const epoch = new Date(0);
console.log("epoch_num=" + Number(epoch));
console.log("epoch_plus=" + +epoch);

const before = new Date(-1);
console.log("before_num=" + Number(before));
const old = new Date(Date.UTC(1900, 0, 1));
console.log("old_num=" + Number(old));
console.log("old_negative=" + (Number(old) < 0));

// Invalid Date coerces to NaN in every numeric form.
const bad = new Date(NaN);
console.log("bad_getTime_nan=" + Number.isNaN(bad.getTime()));
console.log("bad_valueOf_nan=" + Number.isNaN(bad.valueOf()));
console.log("bad_Number_nan=" + Number.isNaN(Number(bad)));
console.log("bad_plus_nan=" + Number.isNaN(+bad));
console.log("bad_arith_nan=" + Number.isNaN((bad as any) - 0));

// Fractional ms is truncated by the constructor.
console.log("frac_ms=" + new Date(1.9).getTime());
console.log("frac_neg_ms=" + new Date(-1.9).getTime());

// Number(date) is an integer.
console.log("is_integer=" + Number.isInteger(Number(d)));

// new Date(number) round-trips through valueOf.
const rt = new Date(Number(d));
console.log("roundtrip=" + (rt.getTime() === d.getTime()));

// Copy-constructing from a Date uses its numeric value.
const copy = new Date(d.getTime());
console.log("copy_eq=" + (copy.getTime() === d.getTime()));
