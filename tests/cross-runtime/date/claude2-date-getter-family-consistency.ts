// ONE thing: every getter answers a field of the SAME instant, and the local
// family must agree with the UTC family under TZ=UTC (which the harness forces).
// A getter derived independently instead of from one broken-down time drifts.
function dump(label: string, d: Date) {
  const parts = [
    d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), d.getUTCDay(),
    d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds(), d.getUTCMilliseconds(),
  ];
  const local = [
    d.getFullYear(), d.getMonth(), d.getDate(), d.getDay(),
    d.getHours(), d.getMinutes(), d.getSeconds(), d.getMilliseconds(),
  ];
  console.log(label + " utc=" + parts.join("/") + " local=" + local.join("/") +
    " agree=" + (parts.join() === local.join()) + " tzoff=" + d.getTimezoneOffset());
}

dump("epoch", new Date(0));
dump("preEpoch", new Date(-1));
dump("y2k", new Date(946684800000));
dump("leapDay", new Date(Date.UTC(2024, 1, 29, 12, 34, 56, 789)));
dump("dayBeforeLeap", new Date(Date.UTC(2024, 1, 28, 23, 59, 59, 999)));
dump("newYearEve", new Date(Date.UTC(1999, 11, 31, 23, 59, 59, 999)));
dump("negYear", new Date(Date.UTC(-1, 0, 1)));
dump("far", new Date(Date.UTC(275760, 8, 13)));

// The weekday cycle must be continuous across the epoch.
const days: string[] = [];
for (let i = -3; i <= 3; i++) days.push(String(new Date(i * 86400000).getUTCDay()));
console.log("weekdayRun=" + days.join(","));

// getTime, valueOf and unary + are the same number.
const d = new Date(1234567890123);
console.log("same=" + (d.getTime() === d.valueOf()) + " " + (+d === d.getTime()) + " v=" + d.getTime());

// An invalid date answers NaN from every getter and from getTime.
const bad = new Date(NaN);
const badVals = [bad.getTime(), bad.getUTCFullYear(), bad.getUTCMonth(), bad.getUTCDay(), bad.getTimezoneOffset()];
console.log("invalidAllNaN=" + badVals.every((v) => Number.isNaN(v)) + " str=" + String(bad.getTime()));
console.log("invalidToJSON=" + JSON.stringify(bad.toJSON()));
try { bad.toISOString(); } catch (e: any) { console.log("invalidISO=" + e.constructor.name); }

// The range boundary: ±8.64e15 is valid, one more is not.
console.log("maxOk=" + new Date(8640000000000000).getTime());
console.log("minOk=" + new Date(-8640000000000000).getTime());
console.log("overMax=" + new Date(8640000000000001).getTime());
console.log("setTimeOver=" + new Date(0).setTime(8640000000000001));

// getYear/setYear are the legacy pair: getYear is year-1900.
const legacy: any = new Date(Date.UTC(2024, 0, 1));
console.log("getYear=" + legacy.getYear() + " fullYear=" + legacy.getFullYear());

// Month and day are zero- and one-based respectively; getDay is zero-based
// from Sunday. Pin the three conventions side by side.
const conv = new Date(Date.UTC(2026, 7, 16, 0, 0, 0));
console.log("conventions=" + conv.getUTCMonth() + "/" + conv.getUTCDate() + "/" + conv.getUTCDay() + " iso=" + conv.toISOString());
