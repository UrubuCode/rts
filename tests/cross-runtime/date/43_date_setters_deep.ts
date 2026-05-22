// Cross-runtime future coverage: Date setters.
const d = new Date(Date.UTC(2024, 0, 1, 0, 0, 0, 0));
d.setUTCMonth(5);
d.setUTCDate(15);
d.setUTCHours(8);
console.log("iso=" + d.toISOString());
console.log("month=" + d.getUTCMonth());
console.log("date=" + d.getUTCDate());
console.log("hours=" + d.getUTCHours());
