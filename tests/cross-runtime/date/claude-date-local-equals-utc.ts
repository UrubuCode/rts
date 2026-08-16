// Cross-runtime: under TZ=UTC (which the harness forces) the local calendar and
// the UTC calendar coincide, so every local getter/setter must agree with its
// UTC twin and getTimezoneOffset must be exactly 0.

const d = new Date(Date.UTC(2024, 5, 10, 7, 8, 9, 10));

console.log("timezone_offset=" + d.getTimezoneOffset());
console.log("offset_is_zero=" + (d.getTimezoneOffset() === 0));

// Every getter pair.
console.log("year=" + d.getFullYear() + "/" + d.getUTCFullYear());
console.log("month=" + d.getMonth() + "/" + d.getUTCMonth());
console.log("date=" + d.getDate() + "/" + d.getUTCDate());
console.log("day=" + d.getDay() + "/" + d.getUTCDay());
console.log("hours=" + d.getHours() + "/" + d.getUTCHours());
console.log("minutes=" + d.getMinutes() + "/" + d.getUTCMinutes());
console.log("seconds=" + d.getSeconds() + "/" + d.getUTCSeconds());
console.log("ms=" + d.getMilliseconds() + "/" + d.getUTCMilliseconds());

function allAgree(x: Date): boolean {
  return x.getFullYear() === x.getUTCFullYear() &&
    x.getMonth() === x.getUTCMonth() &&
    x.getDate() === x.getUTCDate() &&
    x.getDay() === x.getUTCDay() &&
    x.getHours() === x.getUTCHours() &&
    x.getMinutes() === x.getUTCMinutes() &&
    x.getSeconds() === x.getUTCSeconds() &&
    x.getMilliseconds() === x.getUTCMilliseconds();
}
console.log("all_agree=" + allAgree(d));

// Across instants a shifted zone would have separated: midnight, year ends,
// and the two dates a northern-hemisphere DST rule would move.
console.log("agree_epoch=" + allAgree(new Date(0)));
console.log("agree_midnight=" + allAgree(new Date(Date.UTC(2024, 0, 1, 0, 0, 0, 0))));
console.log("agree_year_end=" + allAgree(new Date(Date.UTC(2023, 11, 31, 23, 59, 59, 999))));
console.log("agree_march=" + allAgree(new Date(Date.UTC(2024, 2, 31, 1, 30, 0, 0))));
console.log("agree_october=" + allAgree(new Date(Date.UTC(2024, 9, 27, 1, 30, 0, 0))));
console.log("agree_pre_epoch=" + allAgree(new Date(Date.UTC(1965, 6, 4, 12, 0, 0, 0))));

// A local constructor lands on the same instant as Date.UTC.
console.log("ctor_matches_utc=" + (new Date(2024, 5, 10, 7, 8, 9, 10).getTime() === Date.UTC(2024, 5, 10, 7, 8, 9, 10)));
console.log("ctor_time=" + new Date(2024, 5, 10, 7, 8, 9, 10).getTime());

// A local setter moves the instant by the same amount as its UTC twin.
const a = new Date(Date.UTC(2024, 0, 1));
const b = new Date(Date.UTC(2024, 0, 1));
a.setHours(13);
b.setUTCHours(13);
console.log("setHours_matches=" + (a.getTime() === b.getTime()));
console.log("setHours_iso=" + a.toISOString());

a.setMonth(7, 20);
b.setUTCMonth(7, 20);
console.log("setMonth_matches=" + (a.getTime() === b.getTime()));

a.setFullYear(1999, 11, 31);
b.setUTCFullYear(1999, 11, 31);
console.log("setFullYear_matches=" + (a.getTime() === b.getTime()));
console.log("setFullYear_iso=" + a.toISOString());

// A date-time string WITHOUT an offset is local by spec, which under TZ=UTC is
// the same instant as the Z form.
console.log("naive_equals_z=" + (Date.parse("2024-06-10T07:08:09.010") === Date.parse("2024-06-10T07:08:09.010Z")));
console.log("naive_value=" + Date.parse("2024-06-10T07:08:09.010"));
console.log("date_only_is_utc=" + (Date.parse("2024-06-10") === Date.UTC(2024, 5, 10)));

// A date-only form is UTC while the same day as a naive date-time is local:
// under TZ=UTC they coincide.
console.log("date_only_vs_naive=" + (Date.parse("2024-06-10") === Date.parse("2024-06-10T00:00:00")));

// The spec-pinned string forms.
console.log("to_utc_string=" + d.toUTCString());
console.log("to_date_string=" + d.toDateString());
console.log("epoch_utc_string=" + new Date(0).toUTCString());
console.log("epoch_date_string=" + new Date(0).toDateString());
console.log("single_digit_day=" + new Date(Date.UTC(2024, 0, 5)).toUTCString());
console.log("pre_epoch_utc_string=" + new Date(Date.UTC(1969, 6, 20, 20, 17, 40)).toUTCString());

// Invalid dates print the pinned marker and answer NaN everywhere.
const bad = new Date(NaN);
console.log("invalid_to_string=" + String(bad));
console.log("invalid_date_string=" + bad.toDateString());
console.log("invalid_utc_string=" + bad.toUTCString());
console.log("invalid_offset_nan=" + Number.isNaN(bad.getTimezoneOffset()));
console.log("invalid_getters_nan=" + [bad.getFullYear(), bad.getHours(), bad.getUTCHours()].every(Number.isNaN));

// A round trip through the local getters rebuilds the same instant.
const rebuilt = new Date(
  d.getFullYear(), d.getMonth(), d.getDate(),
  d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds(),
);
console.log("round_trip=" + (rebuilt.getTime() === d.getTime()));
console.log("round_trip_iso=" + rebuilt.toISOString());
