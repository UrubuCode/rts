// Cross-runtime: Date.parse of ISO 8601 forms with Z and with explicit offsets.
// Every datetime here carries an explicit offset (or is a date-only form, which
// the spec fixes to UTC), so the result never depends on the machine timezone.

// Date-only forms are UTC by spec.
console.log("date_only=" + Date.parse("2024-01-15"));
console.log("year_month=" + Date.parse("2024-01"));
console.log("year_only=" + Date.parse("2024"));
console.log("date_only_iso=" + new Date(Date.parse("2024-01-15")).toISOString());

// Z suffix.
console.log("z_basic=" + Date.parse("2024-01-15T10:30:00Z"));
console.log("z_ms=" + Date.parse("2024-01-15T10:30:00.123Z"));
console.log("z_no_sec=" + Date.parse("2024-01-15T10:30Z"));
console.log("z_eq_utc=" + (Date.parse("2024-01-15T10:30:00Z") === Date.UTC(2024, 0, 15, 10, 30, 0)));

// Explicit +00:00 equals Z.
console.log("plus0_eq_z=" + (Date.parse("2024-01-15T10:30:00+00:00") === Date.parse("2024-01-15T10:30:00Z")));
console.log("minus0_eq_z=" + (Date.parse("2024-01-15T10:30:00-00:00") === Date.parse("2024-01-15T10:30:00Z")));

// Positive offsets shift the instant earlier in UTC.
console.log("plus1=" + new Date(Date.parse("2024-01-15T10:30:00+01:00")).toISOString());
console.log("plus0530=" + new Date(Date.parse("2024-01-15T10:30:00+05:30")).toISOString());
console.log("plus14=" + new Date(Date.parse("2024-01-15T10:30:00+14:00")).toISOString());

// Negative offsets shift later in UTC.
console.log("minus3=" + new Date(Date.parse("2024-01-15T10:30:00-03:00")).toISOString());
console.log("minus0930=" + new Date(Date.parse("2024-01-15T10:30:00-09:30")).toISOString());

// Offset arithmetic: +01:00 is exactly one hour before Z.
console.log("offset_delta=" + (Date.parse("2024-01-15T10:30:00Z") - Date.parse("2024-01-15T10:30:00+01:00")));

// An offset can roll the UTC date across a day boundary.
console.log("roll_next_day=" + new Date(Date.parse("2024-01-15T23:30:00-02:00")).toISOString());
console.log("roll_prev_day=" + new Date(Date.parse("2024-01-15T00:30:00+02:00")).toISOString());

// Fractional-second precision.
console.log("ms_1digit=" + new Date(Date.parse("2024-01-15T10:30:00.1Z")).toISOString());
console.log("ms_2digit=" + new Date(Date.parse("2024-01-15T10:30:00.12Z")).toISOString());
console.log("ms_3digit=" + new Date(Date.parse("2024-01-15T10:30:00.123Z")).toISOString());

// Epoch and boundaries.
console.log("epoch=" + Date.parse("1970-01-01T00:00:00.000Z"));
console.log("pre_epoch=" + Date.parse("1969-12-31T23:59:59.999Z"));
console.log("leap_day=" + new Date(Date.parse("2020-02-29T12:00:00Z")).toISOString());

// Round-trip: toISOString output must re-parse to the same instant.
const rt = new Date(Date.UTC(2024, 5, 10, 7, 8, 9, 10));
console.log("roundtrip=" + (Date.parse(rt.toISOString()) === rt.getTime()));

// Malformed inputs => NaN.
console.log("garbage_nan=" + Number.isNaN(Date.parse("not-a-date")));
console.log("empty_nan=" + Number.isNaN(Date.parse("")));
console.log("bad_month_nan=" + Number.isNaN(Date.parse("2024-13-01T00:00:00Z")));
console.log("bad_day_nan=" + Number.isNaN(Date.parse("2024-01-32T00:00:00Z")));
console.log("bad_hour_nan=" + Number.isNaN(Date.parse("2024-01-15T25:00:00Z")));
console.log("nonleap_feb29_nan=" + Number.isNaN(Date.parse("2021-02-29T00:00:00Z")));

// new Date(string) agrees with Date.parse(string).
console.log("ctor_eq_parse=" + (new Date("2024-01-15T10:30:00Z").getTime() === Date.parse("2024-01-15T10:30:00Z")));
