// Cross-runtime: the EXTENDED-YEAR forms of toISOString — a year outside
// 0..9999 is written with a sign and six digits (+275760, -000001), a year
// inside it with four, and Date.parse reads both back.

// `Date.UTC` maps a year in 0..99 to 1900+y, so a small year has to be set
// through setUTCFullYear, which does no remapping at all.
function isoOf(y: number): string {
  const d = new Date(0);
  d.setUTCFullYear(y, 0, 1);
  d.setUTCHours(0, 0, 0, 0);
  return d.toISOString();
}

// Four-digit years are zero-padded.
console.log("y1=" + isoOf(1));
console.log("y12=" + isoOf(12));
console.log("y123=" + isoOf(123));
console.log("y1234=" + isoOf(1234));
console.log("y9999=" + isoOf(9999));
console.log("utc_remaps_small_year=" + new Date(Date.UTC(1, 0, 1)).toISOString());
console.log("setter_does_not_remap=" + isoOf(1));

// Year 0 exists and is written 0000 (there is no "1 BC" gap here).
const zero = new Date(0);
zero.setUTCFullYear(0, 0, 1);
zero.setUTCHours(0, 0, 0, 0);
console.log("year_zero=" + zero.toISOString());
console.log("year_zero_getter=" + zero.getUTCFullYear());

// Negative years take the six-digit signed form.
const neg = new Date(0);
neg.setUTCFullYear(-1, 0, 1);
neg.setUTCHours(0, 0, 0, 0);
console.log("year_minus1=" + neg.toISOString());
console.log("year_minus1_getter=" + neg.getUTCFullYear());

const neg2 = new Date(0);
neg2.setUTCFullYear(-12345, 5, 6);
neg2.setUTCHours(1, 2, 3, 4);
console.log("year_minus12345=" + neg2.toISOString());

// Years above 9999 take the six-digit signed form as well.
const big = new Date(0);
big.setUTCFullYear(10000, 0, 1);
big.setUTCHours(0, 0, 0, 0);
console.log("year_10000=" + big.toISOString());

const bigger = new Date(0);
bigger.setUTCFullYear(123456, 0, 1);
bigger.setUTCHours(0, 0, 0, 0);
console.log("year_123456=" + bigger.toISOString());

// The extremes of the representable range.
const max = new Date(8.64e15);
const min = new Date(-8.64e15);
console.log("max_iso=" + max.toISOString());
console.log("min_iso=" + min.toISOString());
console.log("max_year=" + max.getUTCFullYear());
console.log("min_year=" + min.getUTCFullYear());
console.log("max_time=" + max.getTime());
console.log("min_time=" + min.getTime());

// One millisecond past either extreme is an Invalid Date.
console.log("past_max_nan=" + Number.isNaN(new Date(8.64e15 + 1).getTime()));
console.log("past_min_nan=" + Number.isNaN(new Date(-8.64e15 - 1).getTime()));
console.log("setTime_past_max=" + Number.isNaN(new Date(0).setTime(8.64e15 + 1)));
console.log("setTime_at_max=" + new Date(0).setTime(8.64e15));

// The signed form round-trips through Date.parse.
function roundTrip(d: Date): boolean {
  return Date.parse(d.toISOString()) === d.getTime();
}
console.log("rt_max=" + roundTrip(max));
console.log("rt_min=" + roundTrip(min));
console.log("rt_zero=" + roundTrip(zero));
console.log("rt_neg=" + roundTrip(neg));
console.log("rt_big=" + roundTrip(big));

// Parsing the signed forms directly.
console.log("parse_plus=" + Date.parse("+010000-01-01T00:00:00.000Z"));
console.log("parse_minus=" + Date.parse("-000001-01-01T00:00:00.000Z"));
console.log("parse_zero=" + Date.parse("+000000-01-01T00:00:00.000Z"));
console.log("parse_plus_matches=" + (Date.parse("+010000-01-01T00:00:00.000Z") === big.getTime()));
console.log("parse_minus_matches=" + (Date.parse("-000001-01-01T00:00:00.000Z") === neg.getTime()));

// -000000 is explicitly rejected by the spec.
console.log("minus_zero_year_nan=" + Number.isNaN(Date.parse("-000000-01-01T00:00:00.000Z")));

// The extremes are exactly at the parse boundary.
console.log("parse_max=" + (Date.parse("+275760-09-13T00:00:00.000Z") === 8.64e15));
console.log("parse_min=" + (Date.parse("-271821-04-20T00:00:00.000Z") === -8.64e15));
console.log("parse_past_max_nan=" + Number.isNaN(Date.parse("+275760-09-14T00:00:00.000Z")));

// toJSON follows toISOString for the extended forms, and null past the edge.
console.log("json_max=" + max.toJSON());
console.log("json_min=" + min.toJSON());
console.log("json_neg=" + neg.toJSON());
console.log("json_past_max=" + new Date(8.64e15 + 1).toJSON());
console.log("stringify_neg=" + JSON.stringify({ at: neg }));

// The year getter agrees with the printed form for each shape.
function yearOf(d: Date): string {
  return d.getUTCFullYear() + "|" + d.toISOString().slice(0, d.toISOString().indexOf("-", 1));
}
console.log("shape_zero=" + yearOf(zero));
console.log("shape_neg=" + yearOf(neg));
console.log("shape_big=" + yearOf(big));
console.log("shape_normal=" + yearOf(new Date(Date.UTC(2024, 0, 1))));
