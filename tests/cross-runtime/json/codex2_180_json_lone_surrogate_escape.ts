// Cross-runtime: stringify emits well-formed escapes for lone surrogates.
const high = "\ud800";
const low = "\udc00";
const pair = "\ud83d\ude00";
console.log(JSON.stringify(high));
console.log(JSON.stringify(low));
console.log(JSON.stringify(pair));
console.log(JSON.parse(JSON.stringify(high)).length);

