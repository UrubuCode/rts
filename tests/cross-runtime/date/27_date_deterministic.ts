// Cross-runtime: deterministic Date behavior with UTC values.
const stamp = Date.UTC(2024, 4, 10, 12, 34, 56, 789);
const d = new Date(stamp);

console.log("stamp=" + stamp);
console.log("year=" + d.getUTCFullYear());
console.log("month=" + d.getUTCMonth());
console.log("date=" + d.getUTCDate());
console.log("hours=" + d.getUTCHours());
console.log("iso=" + d.toISOString());
