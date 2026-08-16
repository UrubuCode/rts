// Cross-runtime: split inserts captured separators and respects limit.
const input = "a1b22c";
console.log(JSON.stringify(input.split(/(\d+)/)));
console.log(JSON.stringify(input.split(/(\d+)/, 4)));

