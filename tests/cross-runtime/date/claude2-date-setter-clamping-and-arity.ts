// ONE thing: a Date setter's ARITY decides which fields it touches. Passing
// fewer arguments leaves the lower-order fields alone; passing NaN in any
// position makes the whole date invalid and it never recovers.
function after(label: string, f: (d: Date) => number) {
  const d = new Date(Date.UTC(2024, 5, 15, 10, 20, 30, 400));
  const r = f(d);
  console.log(label + " ret=" + (Number.isNaN(r) ? "NaN" : String(r)) +
    " iso=" + (Number.isNaN(d.getTime()) ? "Invalid" : d.toISOString()));
}

after("setUTCFullYear_1", (d) => d.setUTCFullYear(2030));
after("setUTCFullYear_2", (d) => d.setUTCFullYear(2030, 0));
after("setUTCFullYear_3", (d) => d.setUTCFullYear(2030, 0, 2));
after("setUTCMonth_1", (d) => d.setUTCMonth(0));
after("setUTCMonth_2", (d) => d.setUTCMonth(0, 31));
after("setUTCHours_1", (d) => d.setUTCHours(1));
after("setUTCHours_4", (d) => d.setUTCHours(1, 2, 3, 4));
after("setUTCMinutes_3", (d) => d.setUTCMinutes(1, 2, 3));
after("setUTCSeconds_2", (d) => d.setUTCSeconds(1, 2));
after("setUTCMilliseconds", (d) => d.setUTCMilliseconds(999));

// Overflow ROLLS OVER rather than clamping — month 12 is next January.
after("month12", (d) => d.setUTCMonth(12));
after("month_neg1", (d) => d.setUTCMonth(-1));
after("date0", (d) => d.setUTCDate(0));
after("date32", (d) => d.setUTCDate(32));
after("date_neg5", (d) => d.setUTCDate(-5));
after("hours25", (d) => d.setUTCHours(25));
after("hours_neg1", (d) => d.setUTCHours(-1));
after("ms_huge", (d) => d.setUTCMilliseconds(86400000));
after("seconds_neg", (d) => d.setUTCSeconds(-1));

// NaN anywhere invalidates, and a later valid setter does NOT repair it —
// except setTime and setUTCFullYear, which can rebuild from nothing.
after("nanMonth", (d) => d.setUTCMonth(NaN));
after("nanThenDate", (d) => { d.setUTCMonth(NaN); return d.setUTCDate(1); });
after("nanThenTime", (d) => { d.setUTCMonth(NaN); return d.setTime(0); });
after("nanThenFullYear", (d) => { d.setUTCMonth(NaN); return d.setUTCFullYear(2000, 0, 1); });

// Arguments are coerced with ToNumber, so strings, booleans and objects work.
after("strMonth", (d) => d.setUTCMonth("3" as any));
after("boolMonth", (d) => d.setUTCMonth(true as any));
after("objMonth", (d) => d.setUTCMonth({ valueOf: () => 2 } as any));
after("undefMonth", (d) => d.setUTCMonth(undefined as any));
after("nullMonth", (d) => d.setUTCMonth(null as any));
after("fracHours", (d) => d.setUTCHours(3.9));

// A setter with NO argument reads undefined -> NaN.
after("noArgMonth", (d) => (d.setUTCMonth as any)());

// The setter arities themselves are part of the contract.
console.log("arities=" + [
  Date.prototype.setUTCFullYear.length, Date.prototype.setUTCMonth.length,
  Date.prototype.setUTCDate.length, Date.prototype.setUTCHours.length,
  Date.prototype.setUTCMinutes.length, Date.prototype.setUTCSeconds.length,
  Date.prototype.setUTCMilliseconds.length, Date.prototype.setTime.length,
].join(","));

// Every setter answers the new time value, which equals getTime afterwards.
const d2 = new Date(0);
const ret = d2.setUTCFullYear(2001);
console.log("returnIsTime=" + (ret === d2.getTime()) + " v=" + ret);
