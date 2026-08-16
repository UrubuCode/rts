// Cross-runtime: Date uses a string-like default hint but numeric unary plus.
const date = new Date("2020-01-02T03:04:05.000Z");
console.log(date + "");
console.log(+date);
console.log(date == date.toString(), date == +date);

