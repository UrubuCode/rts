// Cross-runtime: Date.parse
const ts1 = Date.parse("2024-01-01T00:00:00.000Z");
console.log("parse1=" + ts1);

const ts2 = Date.parse("2000-12-31T23:59:59.999Z");
console.log("parse2=" + ts2);

const ts3 = Date.parse("1970-01-01T00:00:00.000Z");
console.log("epoch=" + ts3);

// Invalid
const invalid = Date.parse("invalid");
console.log("invalid=" + isNaN(invalid));
