// Cross-runtime: greedy and lazy quantifiers choose different spans.
const s = "<a><b><c>";
console.log(s.match(/<.*>/)![0]);
console.log(JSON.stringify(s.match(/<.*?>/g)));
console.log("aaaa".match(/a{2,3}/g)!.join("|"));

