// Cross-runtime: Date setUTC* family and getUTC*.
const d = new Date(Date.UTC(2020, 0, 1, 0, 0, 0, 0));
d.setUTCFullYear(2024);
d.setUTCMonth(4);
d.setUTCDate(10);
d.setUTCHours(12);
d.setUTCMinutes(34);
d.setUTCSeconds(56);
d.setUTCMilliseconds(789);
console.log("iso=" + d.toISOString());
console.log("parts=" + [d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds(), d.getUTCMilliseconds()].join(","));
