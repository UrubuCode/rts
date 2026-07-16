// Cross-runtime: new Date(y, m, d, ...) field rollover with out-of-range values.
// Constructed via Date.UTC + new Date(ms) so no local timezone leaks in.

function iso(ms: number): string {
  return new Date(ms).toISOString();
}

// Month overflow rolls the year forward.
console.log("m12=" + iso(Date.UTC(2020, 12, 1)));
console.log("m13=" + iso(Date.UTC(2020, 13, 1)));
console.log("m24=" + iso(Date.UTC(2020, 24, 1)));

// Negative month rolls the year backward.
console.log("m_neg1=" + iso(Date.UTC(2020, -1, 1)));
console.log("m_neg12=" + iso(Date.UTC(2020, -12, 1)));
console.log("m_neg13=" + iso(Date.UTC(2020, -13, 1)));

// Day 0 is the last day of the previous month.
console.log("d0_jan=" + iso(Date.UTC(2020, 0, 0)));
console.log("d0_mar=" + iso(Date.UTC(2020, 2, 0)));

// Day overflow past month length rolls into the next month.
console.log("jan32=" + iso(Date.UTC(2020, 0, 32)));
console.log("feb30_leap=" + iso(Date.UTC(2020, 1, 30)));
console.log("feb30_nonleap=" + iso(Date.UTC(2021, 1, 30)));
console.log("apr31=" + iso(Date.UTC(2020, 3, 31)));

// Negative day counts backward from day 0.
console.log("d_neg1=" + iso(Date.UTC(2020, 0, -1)));

// Huge day overflow spans years.
console.log("d366=" + iso(Date.UTC(2020, 0, 366)));
console.log("d367=" + iso(Date.UTC(2020, 0, 367)));

// Time fields roll into the date.
console.log("h24=" + iso(Date.UTC(2020, 0, 1, 24)));
console.log("h_neg1=" + iso(Date.UTC(2020, 0, 1, -1)));
console.log("min60=" + iso(Date.UTC(2020, 0, 1, 0, 60)));
console.log("sec60=" + iso(Date.UTC(2020, 0, 1, 0, 0, 60)));
console.log("ms1000=" + iso(Date.UTC(2020, 0, 1, 0, 0, 0, 1000)));
console.log("ms_neg1=" + iso(Date.UTC(2020, 0, 1, 0, 0, 0, -1)));

// Cascading rollover across every field at once.
console.log("cascade=" + iso(Date.UTC(2020, 11, 31, 23, 59, 59, 1000)));

// Leap-day boundaries.
console.log("leap2020=" + iso(Date.UTC(2020, 1, 29)));
console.log("leap1900=" + iso(Date.UTC(1900, 1, 29)));
console.log("leap2000=" + iso(Date.UTC(2000, 1, 29)));
