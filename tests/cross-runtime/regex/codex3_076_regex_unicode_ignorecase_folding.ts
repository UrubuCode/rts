// Cross-runtime: Unicode ignore-case matching handles non-ASCII simple folds.
const pairs: Array<[RegExp, string]> = [
  [/k/iu, "K"],
  [/s/iu, "ſ"],
  [/å/iu, "Å"],
  [/ß/iu, "SS"],
];
console.log(pairs.map(([re, s]) => re.test(s)).join(","));

