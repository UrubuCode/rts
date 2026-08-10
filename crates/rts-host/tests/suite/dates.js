// `Date` — a time value, and the civil calendar over it. Everything is UTC.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

let epoch = new Date(0);
check("get-time", epoch.getTime() === 0);
check("value-of", epoch.valueOf() === 0);
check("year", epoch.getFullYear() === 1970);
check("month", epoch.getMonth() === 0);
check("date", epoch.getDate() === 1);
// 1 January 1970 was a Thursday. A conversion written as division alone gets
// the weekday wrong for exactly the dates nobody checks.
check("day", epoch.getDay() === 4);
check("hours", epoch.getHours() === 0);
check("iso", epoch.toISOString() === "1970-01-01T00:00:00.000Z");

check("ms", new Date(1577934245006).getMilliseconds() === 6);
check("seconds", new Date(1577934245006).getSeconds() === 5);
check("minutes", new Date(1577934245006).getMinutes() === 4);

// A leap day, and a time before the epoch — the two the civil conversion gets
// wrong when it is written for positive values only.
check("leap-day", new Date(951782400000).toISOString() === "2000-02-29T00:00:00.000Z");
check("before-epoch-year", new Date(-14182940000).getFullYear() === 1969);
check("before-epoch-hour", new Date(-14182940000).getHours() === 20);

check("parse", Date.parse("2020-01-02T03:04:05.006Z") === 1577934245006);
check("parse-date-only", Date.parse("2020-01-02") === 1577923200000);
check("parse-bad", isNaN(Date.parse("not a date")));
check("from-string", new Date("2020-01-02T03:04:05.006Z").getMonth() === 0);
check("from-number", new Date(1000).getTime() === 1000);
check("invalid", isNaN(new Date("nope").getTime()));

check("utc-static", Date.UTC(1970, 0) === 0);
check("now-is-recent", Date.now() > 1700000000000);
check("now-is-not-future", Date.now() < 4000000000000);

// Everything is UTC, said as a check rather than only in a comment: there is
// no timezone database here, and local time invented from nothing is a wrong
// answer that looks right.
check("timezone-offset", epoch.getTimezoneOffset() === 0);
check("hours-are-utc", (function () {
    let d = new Date(3600000);
    return d.getHours() === d.getUTCHours() && d.getHours() === 1;
})());
check("date-is-utc", (function () {
    let d = new Date(0);
    return d.getDate() === d.getUTCDate();
})());

let moved = new Date(0);
moved.setTime(1000);
check("set-time", moved.getTime() === 1000);

check("to-json", new Date(0).toJSON() === "1970-01-01T00:00:00.000Z");

// This pinned TWO stated divergences meeting: `JSON.stringify` did not consult
// `toJSON`, and the time value is a real property rather than an internal slot,
// so a date serialised as an object holding that property. The first is fixed —
// the hook runs — and the second no longer shows, because `toJSON` answers
// before the walk ever reaches the properties.
//
// The property divergence is still real and still visible: `Object.keys` on a
// date lists it, where a real engine lists nothing. Pinned there instead, so
// fixing it stays a visible change.
check("json-runs-to-json", JSON.stringify(new Date(0)) === "\"1970-01-01T00:00:00.000Z\"");
check("time-is-a-property-divergence", Object.keys(new Date(0)).length === 1);
check("time-value-is-visible", Object.keys(new Date(0)).length === 1);

return failed;
