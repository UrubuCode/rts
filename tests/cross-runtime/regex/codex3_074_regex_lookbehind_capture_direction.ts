// Cross-runtime: quantified captures inside lookbehind reflect reverse matching direction.
const match = /(?<=([ab]+)([bc]+))$/.exec("abc")!;
console.log(match[0], match[1], match[2], match.index);
const negative = /(?<!foo)bar/.exec("xxbar")!;
console.log(negative[0], negative.index);

