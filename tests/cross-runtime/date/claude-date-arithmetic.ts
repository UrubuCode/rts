// Cross-runtime: date arithmetic — ms differences and setUTCDate-driven advance.
// All instants are fixed UTC constants.

const a = new Date(Date.UTC(2024, 0, 1, 0, 0, 0, 0));
const b = new Date(Date.UTC(2024, 0, 2, 0, 0, 0, 0));

// Difference in ms via valueOf coercion.
console.log("diff_ms=" + (b.getTime() - a.getTime()));
console.log("diff_minus_op=" + ((b as any) - (a as any)));
console.log("diff_days=" + (b.getTime() - a.getTime()) / 86400000);
console.log("reverse=" + (a.getTime() - b.getTime()));

// Difference across a leap day.
const feb28 = new Date(Date.UTC(2020, 1, 28));
const mar1 = new Date(Date.UTC(2020, 2, 1));
console.log("leap_gap_days=" + (mar1.getTime() - feb28.getTime()) / 86400000);

const feb28n = new Date(Date.UTC(2021, 1, 28));
const mar1n = new Date(Date.UTC(2021, 2, 1));
console.log("nonleap_gap_days=" + (mar1n.getTime() - feb28n.getTime()) / 86400000);

// Full-year spans.
console.log("year2020_days=" + (Date.UTC(2021, 0, 1) - Date.UTC(2020, 0, 1)) / 86400000);
console.log("year2021_days=" + (Date.UTC(2022, 0, 1) - Date.UTC(2021, 0, 1)) / 86400000);

// Advance a date one day at a time across a month boundary via setUTCDate.
const walk = new Date(Date.UTC(2024, 0, 30));
const seen: string[] = [];
for (let i = 0; i < 4; i++) {
  seen.push(walk.toISOString().slice(0, 10));
  walk.setUTCDate(walk.getUTCDate() + 1);
}
console.log("walk=" + seen.join("|"));

// Advance across a year boundary.
const yearEnd = new Date(Date.UTC(2023, 11, 31));
yearEnd.setUTCDate(yearEnd.getUTCDate() + 1);
console.log("year_roll=" + yearEnd.toISOString());

// Add 45 days in one shot.
const add45 = new Date(Date.UTC(2024, 0, 1));
add45.setUTCDate(add45.getUTCDate() + 45);
console.log("add45=" + add45.toISOString());

// Subtract days below 1.
const sub = new Date(Date.UTC(2024, 2, 5));
sub.setUTCDate(sub.getUTCDate() - 10);
console.log("sub10=" + sub.toISOString());

// Month arithmetic clamps-by-rollover, not by clamping (Jan 31 + 1 month => Mar 2/3).
const monthAdd = new Date(Date.UTC(2024, 0, 31));
monthAdd.setUTCMonth(monthAdd.getUTCMonth() + 1);
console.log("jan31_plus_month=" + monthAdd.toISOString());

// setTime-based arithmetic.
const t = new Date(Date.UTC(2024, 0, 1));
t.setTime(t.getTime() + 3600000);
console.log("plus_hour=" + t.toISOString());
console.log("setTime_returns=" + t.setTime(0));
console.log("after_setTime0=" + t.toISOString());
