// Cross-runtime: invalid dates can be revived by numeric setters.
const d = new Date(NaN);
console.log(Number.isNaN(d.getTime()));
d.setFullYear(2020, 0, 2);
console.log(Number.isNaN(d.getTime()));
console.log(d.getFullYear() + "-" + d.getMonth() + "-" + d.getDate());

const e = new Date(0);
console.log(Number.isNaN(e.setMonth(NaN)));
console.log(String(e));
