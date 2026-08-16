// Cross-runtime: the UTC setters take MORE than one field. Extra arguments fill
// the smaller units, omitted ones keep the current value, extras beyond the
// documented arity are ignored, and every setter returns the new time value.

function iso(d: Date): string { return d.toISOString(); }
function fresh(): Date { return new Date(Date.UTC(2024, 5, 10, 7, 8, 9, 10)); }

// setUTCFullYear(year, month, date)
let d = fresh();
console.log("year_only=" + iso((d.setUTCFullYear(1999), d)));
d = fresh();
console.log("year_month=" + iso((d.setUTCFullYear(1999, 0), d)));
d = fresh();
console.log("year_month_date=" + iso((d.setUTCFullYear(1999, 0, 31), d)));
d = fresh();
console.log("year_extra_ignored=" + iso(((d.setUTCFullYear as any)(1999, 0, 31, 23), d)));

// setUTCMonth(month, date)
d = fresh();
console.log("month_only=" + iso((d.setUTCMonth(11), d)));
d = fresh();
console.log("month_date=" + iso((d.setUTCMonth(11, 25), d)));
d = fresh();
console.log("month_rollover=" + iso((d.setUTCMonth(13, 40), d)));

// setUTCDate(date)
d = fresh();
console.log("date_only=" + iso((d.setUTCDate(1), d)));
d = fresh();
console.log("date_zero=" + iso((d.setUTCDate(0), d)));

// setUTCHours(hours, min, sec, ms)
d = fresh();
console.log("hours_only=" + iso((d.setUTCHours(0), d)));
d = fresh();
console.log("hours_min=" + iso((d.setUTCHours(0, 1), d)));
d = fresh();
console.log("hours_min_sec=" + iso((d.setUTCHours(0, 1, 2), d)));
d = fresh();
console.log("hours_all=" + iso((d.setUTCHours(0, 1, 2, 3), d)));
d = fresh();
console.log("hours_rollover=" + iso((d.setUTCHours(24, 60, 60, 1000), d)));

// setUTCMinutes(min, sec, ms)
d = fresh();
console.log("minutes_only=" + iso((d.setUTCMinutes(30), d)));
d = fresh();
console.log("minutes_sec_ms=" + iso((d.setUTCMinutes(30, 31, 32), d)));

// setUTCSeconds(sec, ms)
d = fresh();
console.log("seconds_only=" + iso((d.setUTCSeconds(45), d)));
d = fresh();
console.log("seconds_ms=" + iso((d.setUTCSeconds(45, 46), d)));

// setUTCMilliseconds(ms)
d = fresh();
console.log("ms_only=" + iso((d.setUTCMilliseconds(999), d)));
d = fresh();
console.log("ms_overflow=" + iso((d.setUTCMilliseconds(1500), d)));

// Every setter returns the new time value, which equals getTime().
d = fresh();
const returned = d.setUTCHours(5, 6, 7, 8);
console.log("returns_time_value=" + (returned === d.getTime()));
console.log("returned_value=" + returned);
console.log("setTime_returns=" + d.setTime(0));
console.log("setDate_returns_number=" + (typeof fresh().setUTCDate(1)));

// An explicit `undefined` in a trailing slot poisons the date, unlike omitting.
d = fresh();
(d.setUTCHours as any)(1, undefined);
console.log("explicit_undefined_nan=" + Number.isNaN(d.getTime()));
d = fresh();
d.setUTCHours(1);
console.log("omitted_is_fine=" + iso(d));

// NaN in any slot poisons it and the setter returns NaN.
d = fresh();
console.log("nan_returns_nan=" + Number.isNaN(d.setUTCMinutes(NaN)));
console.log("nan_stays=" + Number.isNaN(d.getTime()));

// A poisoned date is revived by a full setUTCFullYear, which treats the time
// value as +0 when it is NaN.
d = new Date(NaN);
d.setUTCFullYear(2020, 1, 29);
console.log("revived=" + iso(d));
console.log("revived_time_fields=" + d.getUTCHours() + ":" + d.getUTCMinutes() + ":" + d.getUTCSeconds());

// The other setters cannot revive it.
d = new Date(NaN);
d.setUTCMonth(5, 10);
console.log("month_cannot_revive=" + Number.isNaN(d.getTime()));
d = new Date(NaN);
d.setUTCHours(1, 2, 3, 4);
console.log("hours_cannot_revive=" + Number.isNaN(d.getTime()));

// Fractional arguments truncate toward zero before they are applied.
d = fresh();
console.log("fraction_truncates=" + iso((d.setUTCHours(5.9, 6.9, 7.9, 8.9), d)));
d = fresh();
console.log("negative_fraction=" + iso((d.setUTCMinutes(-1.5), d)));

// A string argument is coerced with ToNumber.
d = fresh();
console.log("string_arg=" + iso((d.setUTCDate("15" as any), d)));
d = fresh();
console.log("bad_string_nan=" + Number.isNaN(d.setUTCDate("nope" as any)));

// Local setters carry the same arity, and under TZ=UTC land on the same value.
const localOne = fresh();
const utcOne = fresh();
localOne.setHours(3, 4, 5, 6);
utcOne.setUTCHours(3, 4, 5, 6);
console.log("local_arity_matches=" + (localOne.getTime() === utcOne.getTime()));
const localTwo = fresh();
const utcTwo = fresh();
localTwo.setFullYear(2001, 8, 11);
utcTwo.setUTCFullYear(2001, 8, 11);
console.log("local_year_matches=" + (localTwo.getTime() === utcTwo.getTime()));
console.log("local_year_iso=" + iso(localTwo));
