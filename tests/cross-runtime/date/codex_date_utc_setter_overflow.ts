// Cross-runtime: UTC setters overflow deterministically.
const d = new Date(Date.UTC(2021, 0, 31, 23, 0, 0));
d.setUTCMonth(1);
console.log(d.toISOString());
d.setUTCDate(0);
console.log(d.toISOString());
d.setUTCHours(48, 120, 120, 1000);
console.log(d.toISOString());
