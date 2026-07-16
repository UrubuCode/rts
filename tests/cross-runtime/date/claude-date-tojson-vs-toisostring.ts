// Cross-runtime: toJSON vs toISOString on valid AND invalid dates.
// Rule: toJSON on a non-finite date returns null; toISOString throws RangeError.

const valid = new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123));
console.log("valid_iso=" + valid.toISOString());
console.log("valid_json=" + valid.toJSON());
console.log("valid_same=" + (valid.toJSON() === valid.toISOString()));

// JSON.stringify routes through toJSON.
console.log("stringify=" + JSON.stringify(valid));
console.log("stringify_nested=" + JSON.stringify({ at: valid }));
console.log("stringify_array=" + JSON.stringify([valid, valid]));

// Invalid date: toJSON => null.
const invalid = new Date(NaN);
console.log("invalid_json=" + invalid.toJSON());
console.log("invalid_json_is_null=" + (invalid.toJSON() === null));
console.log("invalid_stringify=" + JSON.stringify(invalid));
console.log("invalid_nested=" + JSON.stringify({ at: invalid }));
console.log("invalid_in_array=" + JSON.stringify([invalid]));

// Invalid date: toISOString throws RangeError.
try {
  invalid.toISOString();
  console.log("iso_threw=false");
} catch (e) {
  console.log("iso_threw=true");
  console.log("iso_err_is_range=" + (e instanceof RangeError));
  console.log("iso_err_name=" + (e as Error).name);
}

// Same for a date built from an unparseable string.
const bad = new Date("not-a-date");
console.log("bad_json_null=" + (bad.toJSON() === null));
try {
  bad.toISOString();
  console.log("bad_iso_threw=false");
} catch (e) {
  console.log("bad_iso_threw=true");
  console.log("bad_iso_name=" + (e as Error).name);
}

// Out-of-range date (beyond +-8.64e15) is also invalid.
const oor = new Date(8.64e15 + 1);
console.log("oor_nan=" + Number.isNaN(oor.getTime()));
console.log("oor_json_null=" + (oor.toJSON() === null));

// Extreme but legal boundary values still serialize.
const maxDate = new Date(8.64e15);
console.log("max_iso=" + maxDate.toISOString());
console.log("max_json=" + maxDate.toJSON());
const minDate = new Date(-8.64e15);
console.log("min_iso=" + minDate.toISOString());

// Negative-year and 5-digit-year expanded ISO forms.
console.log("neg_year_iso=" + new Date(Date.UTC(-1, 0, 1)).toISOString());
console.log("big_year_iso=" + new Date(Date.UTC(12345, 0, 1)).toISOString());
