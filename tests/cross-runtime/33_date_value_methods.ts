// Cross-runtime: Date value extraction methods.
const d = new Date(Date.UTC(2024, 5, 1, 0, 0, 0, 0));
console.log("time=" + d.getTime());
console.log("json=" + d.toJSON());
console.log("value=" + d.valueOf());
