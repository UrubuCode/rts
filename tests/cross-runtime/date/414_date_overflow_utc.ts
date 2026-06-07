// Cross-runtime: Date UTC overflow and invalid date behavior.
const d = new Date(Date.UTC(2020, 12, 32, 25, 61, 61, 1000));
console.log(d.toISOString());
console.log(d.getUTCFullYear() + "-" + d.getUTCMonth() + "-" + d.getUTCDate());

const invalid = new Date("not-a-date");
console.log(String(invalid));
console.log(Number.isNaN(invalid.getTime()));
