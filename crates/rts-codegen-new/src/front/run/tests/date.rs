//! `Date` (P5.16) — REAL `.ts` run end to end, exact captured stdout.
//!
//! `Date` is a RUNTIME/Registry class (a `#[rtse::class("Date")]` struct stored
//! via the generic `Entry::Rtse`): `new Date(...)` boxes a real handle, methods +
//! `Date.now`/`Date.UTC`/`Date.parse` statics dispatch through the Registry
//! (mirror of the Map/Set/RegExp P5.3/P5.12 pattern).
//!
//! ms is stored in UTC, so every assertion here uses a FIXED, TZ-independent value
//! (epoch ms / `Date.UTC` / UTC getters / `toISOString`) — these match bun
//! deterministically regardless of the machine timezone. The bails pin the honesty
//! floor (TZ-dependent / unmodeled constructs refuse, never a wrong value).

use super::assert_stdout;

// ---------------------------------------------------------------------------
// getTime / valueOf — the stored epoch ms (deterministic).
// ---------------------------------------------------------------------------

#[test]
fn get_time_epoch() {
    assert_stdout(r#"let d = new Date(0); console.log(d.getTime());"#, "0\n");
}

#[test]
fn get_time_1000() {
    assert_stdout(
        r#"let d = new Date(1000); console.log(d.getTime());"#,
        "1000\n",
    );
}

#[test]
fn value_of() {
    assert_stdout(
        r#"let d = new Date(500); console.log(d.valueOf());"#,
        "500\n",
    );
}

#[test]
fn get_time_large_ms() {
    // A value past i32 range rides the f64 number path; ToString prints the
    // integer with no decimals.
    assert_stdout(
        r#"let d = new Date(1577836800000); console.log(d.getTime());"#,
        "1577836800000\n",
    );
}

// ---------------------------------------------------------------------------
// toISOString / toJSON — deterministic UTC string.
// ---------------------------------------------------------------------------

#[test]
fn to_iso_string_epoch() {
    assert_stdout(
        r#"let d = new Date(0); console.log(d.toISOString());"#,
        "1970-01-01T00:00:00.000Z\n",
    );
}

#[test]
fn to_json_epoch() {
    assert_stdout(
        r#"let d = new Date(0); console.log(d.toJSON());"#,
        "1970-01-01T00:00:00.000Z\n",
    );
}

#[test]
fn new_from_iso_round_trips() {
    assert_stdout(
        r#"let d = new Date("2021-06-15T00:00:00.000Z"); console.log(d.toISOString());"#,
        "2021-06-15T00:00:00.000Z\n",
    );
}

// ---------------------------------------------------------------------------
// Date.UTC (static) — ms since epoch, deterministic.
// ---------------------------------------------------------------------------

#[test]
fn date_utc_2020_jan_1() {
    assert_stdout(r#"console.log(Date.UTC(2020, 0, 1));"#, "1577836800000\n");
}

#[test]
fn date_utc_round_trips_through_new() {
    assert_stdout(
        r#"let d = new Date(Date.UTC(2021, 5, 15)); console.log(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());"#,
        "2021 5 15\n",
    );
}

// ---------------------------------------------------------------------------
// UTC getters — deterministic (ms stored in UTC).
// ---------------------------------------------------------------------------

#[test]
fn get_utc_full_year_epoch() {
    assert_stdout(
        r#"let d = new Date(0); console.log(d.getUTCFullYear());"#,
        "1970\n",
    );
}

#[test]
fn get_utc_date_one_day_after_epoch() {
    assert_stdout(
        r#"let d = new Date(86400000); console.log(d.getUTCDate());"#,
        "2\n",
    );
}

#[test]
fn get_utc_components_full() {
    assert_stdout(
        r#"let d = new Date(Date.UTC(2021, 5, 15, 13, 30, 45, 500));
console.log(d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds(), d.getUTCMilliseconds());"#,
        "13 30 45 500\n",
    );
}

#[test]
fn get_utc_day_of_week() {
    // 2021-06-15 is a Tuesday → getUTCDay() == 2.
    assert_stdout(
        r#"let d = new Date(Date.UTC(2021, 5, 15)); console.log(d.getUTCDay());"#,
        "2\n",
    );
}

// ---------------------------------------------------------------------------
// Local getters reuse the UTC externs (RTS stores UTC) — byte-identical to a
// UTC-running bun's getUTC*. Asserted on a UTC-built instance.
// ---------------------------------------------------------------------------

#[test]
fn local_getters_alias_utc_on_utc_instance() {
    assert_stdout(
        r#"let d = new Date(Date.UTC(2021, 5, 15)); console.log(d.getFullYear(), d.getMonth(), d.getDate());"#,
        "2021 5 15\n",
    );
}

// ---------------------------------------------------------------------------
// Date.parse (static).
// ---------------------------------------------------------------------------

#[test]
fn date_parse_iso() {
    assert_stdout(
        r#"console.log(Date.parse("1970-01-01T00:00:01.000Z"));"#,
        "1000\n",
    );
}

// ---------------------------------------------------------------------------
// typeof / instanceof.
// ---------------------------------------------------------------------------

#[test]
fn typeof_date_is_object() {
    assert_stdout(r#"console.log(typeof new Date(0));"#, "object\n");
}

#[test]
fn instanceof_date_true() {
    assert_stdout(r#"console.log(new Date(0) instanceof Date);"#, "true\n");
}

#[test]
fn non_date_not_instanceof_date() {
    assert_stdout(
        r#"let d = new Date(0); console.log((5) instanceof Date);"#,
        "false\n",
    );
}

// ---------------------------------------------------------------------------
// Deterministic UTC surface — setters mutate in place; formatters emit the
// UTC-deterministic forms (the modeled semantic; `MemberFlags::UNSOUND` was
// dropped from the Date spec on 2026-07-02, the TS suite defines these).
// ---------------------------------------------------------------------------

#[test]
fn set_time_mutates() {
    assert_stdout(
        r#"let d = new Date(0); d.setTime(1000); console.log(d.getTime());"#,
        "1000\n",
    );
}

#[test]
fn set_full_year_mutates() {
    assert_stdout(
        r#"let d = new Date(0); d.setFullYear(2024); console.log(d.getUTCFullYear());"#,
        "2024\n",
    );
}

#[test]
fn to_string_js_spec_utc() {
    assert_stdout(
        r#"let d = new Date(0); console.log(d.toString());"#,
        "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)\n",
    );
}

#[test]
fn to_date_string_utc() {
    assert_stdout(
        r#"let d = new Date(0); console.log(d.toDateString());"#,
        "Thu Jan 01 1970\n",
    );
}
