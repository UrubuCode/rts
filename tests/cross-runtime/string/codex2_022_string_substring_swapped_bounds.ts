// Cross-runtime: substring clamps negatives and swaps reversed bounds.
const s = "0123456789";
console.log([s.substring(7, 2), s.substring(-3, 4), s.substring(4, 99)].join("|"));
console.log(s.substring(3.8, 6.9));

