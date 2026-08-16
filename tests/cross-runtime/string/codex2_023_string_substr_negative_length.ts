// Cross-runtime: substr uses a negative start and treats negative length as empty.
const s = "cross-runtime";
console.log([s.substr(-7), s.substr(-7, 3), s.substr(2, -1)].join("|"));
console.log(s.substr(-99, 5));

