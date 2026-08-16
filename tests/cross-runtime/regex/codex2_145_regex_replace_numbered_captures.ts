// Cross-runtime: replacement strings expand numbered capture groups.
const input = "Doe, Jane; Smith, John";
console.log(input.replace(/(\w+), (\w+)/g, "$2 $1"));
console.log("abc".replace(/(a)(b)(c)/, "$3$2$1-$4"));

