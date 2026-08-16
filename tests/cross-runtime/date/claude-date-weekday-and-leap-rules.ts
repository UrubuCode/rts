// Cross-runtime: the proleptic Gregorian calendar the Date object implements —
// weekday numbering, the 4/100/400 leap rule (1900 is not a leap year, 2000 is)
// and month lengths derived from "day 0 of the next month".

function isoDay(y: number, m: number, d: number): string {
  return new Date(Date.UTC(y, m, d)).toISOString().slice(0, 10);
}
function weekday(y: number, m: number, d: number): number {
  return new Date(Date.UTC(y, m, d)).getUTCDay();
}

// Sunday is 0. The epoch was a Thursday.
console.log("epoch_day=" + new Date(0).getUTCDay());
console.log("week_of_epoch=" + [1, 2, 3, 4, 5, 6, 7].map((d) => weekday(1970, 0, d)).join(","));

// Known anchors.
console.log("y2000_jan1=" + weekday(2000, 0, 1));
console.log("y2024_jan1=" + weekday(2024, 0, 1));
console.log("y2024_feb29=" + weekday(2024, 1, 29));
console.log("y1969_jul20=" + weekday(1969, 6, 20));
console.log("y1900_jan1=" + weekday(1900, 0, 1));
console.log("y1800_jan1=" + weekday(1800, 0, 1));

// The weekday cycles with period 7 and never leaves 0..6.
const cycle: number[] = [];
for (let i = 0; i < 9; i++) cycle.push(weekday(2024, 0, 1 + i));
console.log("cycle=" + cycle.join(","));
console.log("period7=" + (weekday(2024, 0, 1) === weekday(2024, 0, 8)));

// Leap rule: divisible by 4, except centuries not divisible by 400.
function isLeap(y: number): boolean {
  return new Date(Date.UTC(y, 1, 29)).getUTCMonth() === 1;
}
console.log("leap_2024=" + isLeap(2024));
console.log("leap_2023=" + isLeap(2023));
console.log("leap_2000=" + isLeap(2000));
console.log("leap_1900=" + isLeap(1900));
console.log("leap_2100=" + isLeap(2100));
console.log("leap_2400=" + isLeap(2400));
console.log("leap_1600=" + isLeap(1600));

// Feb 29 in a non-leap year rolls into March 1.
console.log("feb29_1900=" + isoDay(1900, 1, 29));
console.log("feb29_2100=" + isoDay(2100, 1, 29));
console.log("feb29_2000=" + isoDay(2000, 1, 29));

// Month lengths via day 0 of the next month.
const lengths: number[] = [];
for (let m = 0; m < 12; m++) lengths.push(new Date(Date.UTC(2024, m + 1, 0)).getUTCDate());
console.log("lengths_2024=" + lengths.join(","));
const lengths23: number[] = [];
for (let m = 0; m < 12; m++) lengths23.push(new Date(Date.UTC(2023, m + 1, 0)).getUTCDate());
console.log("lengths_2023=" + lengths23.join(","));

// Days in a year, counted by subtraction and by day-of-year.
function daysInYear(y: number): number {
  return (Date.UTC(y + 1, 0, 1) - Date.UTC(y, 0, 1)) / 86400000;
}
console.log("days_2024=" + daysInYear(2024));
console.log("days_2023=" + daysInYear(2023));
console.log("days_1900=" + daysInYear(1900));
console.log("days_2000=" + daysInYear(2000));

// Day of year for the last day of each year.
function dayOfYear(y: number, m: number, d: number): number {
  return (Date.UTC(y, m, d) - Date.UTC(y, 0, 1)) / 86400000 + 1;
}
console.log("doy_2024_dec31=" + dayOfYear(2024, 11, 31));
console.log("doy_2023_dec31=" + dayOfYear(2023, 11, 31));
console.log("doy_2024_mar1=" + dayOfYear(2024, 2, 1));
console.log("doy_2023_mar1=" + dayOfYear(2023, 2, 1));

// A 400-year cycle has a whole number of weeks, so the calendar repeats.
console.log("cycle400_days=" + (Date.UTC(2400, 0, 1) - Date.UTC(2000, 0, 1)) / 86400000);
console.log("cycle400_weeks=" + ((Date.UTC(2400, 0, 1) - Date.UTC(2000, 0, 1)) / 86400000) % 7);
console.log("same_weekday_400=" + (weekday(2000, 0, 1) === weekday(2400, 0, 1)));

// The 13th falls on a Friday somewhere in every year: count them in 2024.
let fridays = 0;
for (let m = 0; m < 12; m++) if (weekday(2024, m, 13) === 5) fridays += 1;
console.log("friday13_in_2024=" + fridays);

// Walk a leap day one day at a time.
const walk: string[] = [];
const cursor = new Date(Date.UTC(2024, 1, 27));
for (let i = 0; i < 5; i++) {
  walk.push(cursor.toISOString().slice(5, 10) + "(" + cursor.getUTCDay() + ")");
  cursor.setUTCDate(cursor.getUTCDate() + 1);
}
console.log("leap_walk=" + walk.join(","));

// The same walk in a non-leap year skips the 29th.
const walk23: string[] = [];
const cursor23 = new Date(Date.UTC(2023, 1, 27));
for (let i = 0; i < 5; i++) {
  walk23.push(cursor23.toISOString().slice(5, 10));
  cursor23.setUTCDate(cursor23.getUTCDate() + 1);
}
console.log("nonleap_walk=" + walk23.join(","));

// Adding a year to Feb 29 by setUTCFullYear rolls to March 1.
const anniversary = new Date(Date.UTC(2024, 1, 29));
anniversary.setUTCFullYear(2025);
console.log("feb29_plus_year=" + anniversary.toISOString().slice(0, 10));

// Adding four years lands back on Feb 29.
const four = new Date(Date.UTC(2024, 1, 29));
four.setUTCFullYear(2028);
console.log("feb29_plus_four=" + four.toISOString().slice(0, 10));

// Weekday is stable across the pre-epoch boundary.
console.log("pre_epoch_walk=" + [-3, -2, -1, 0, 1].map((n) => new Date(n * 86400000).getUTCDay()).join(","));
