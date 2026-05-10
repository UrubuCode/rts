// Cross-runtime: Date getters (deterministic)
const d = new Date(Date.UTC(2024, 0, 15, 10, 30, 45, 123));

console.log("utc_year=" + d.getUTCFullYear());
console.log("utc_month=" + d.getUTCMonth());
console.log("utc_date=" + d.getUTCDate());
console.log("utc_hours=" + d.getUTCHours());
console.log("utc_minutes=" + d.getUTCMinutes());
console.log("utc_seconds=" + d.getUTCSeconds());
console.log("utc_ms=" + d.getUTCMilliseconds());
console.log("utc_day=" + d.getUTCDay());
