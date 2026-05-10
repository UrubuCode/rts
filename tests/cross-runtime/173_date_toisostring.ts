// Cross-runtime: Date.toISOString
const d1 = new Date(Date.UTC(2024, 0, 1, 12, 30, 45, 123));
console.log("iso=" + d1.toISOString());

const d2 = new Date(Date.UTC(2000, 11, 31, 23, 59, 59, 999));
console.log("iso2=" + d2.toISOString());

const d3 = new Date(Date.UTC(1970, 0, 1, 0, 0, 0, 0));
console.log("epoch=" + d3.toISOString());

// toJSON uses toISOString
console.log("json=" + d1.toJSON());
