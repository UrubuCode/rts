// `Array.prototype` and the statics beside it.
let failed = "";
function check(name, held) { if (!held) { failed = failed + name + ","; } }

let a = [1, 2, 3];
check("length", a.length === 3);
check("index", a[0] === 1);
check("past-end", a[9] === undefined);
check("negative-at", a.at(-1) === 3);
check("index-of", a.indexOf(2) === 1);
check("index-of-missing", a.indexOf(9) === -1);
check("last-index-of", [1, 2, 1].lastIndexOf(1) === 2);
check("includes", a.includes(2));
check("join", a.join("-") === "1-2-3");
check("join-default", a.join() === "1,2,3");
check("to-string", a.toString() === "1,2,3");
check("slice", a.slice(1).length === 2);
check("slice-negative", a.slice(-1)[0] === 3);
check("concat", a.concat([4]).length === 4);

let grown = [1];
grown.push(2);
check("push", grown.length === 2 && grown[1] === 2);
check("pop", grown.pop() === 2 && grown.length === 1);
grown.unshift(0);
check("unshift", grown[0] === 0 && grown.length === 2);
check("shift", grown.shift() === 0 && grown.length === 1);

check("map", [1, 2].map(function (x) { return x * 2; })[1] === 4);
check("filter", [1, 2, 3].filter(function (x) { return x > 1; }).length === 2);
check("find", [1, 2, 3].find(function (x) { return x > 1; }) === 2);
check("find-index", [1, 2, 3].findIndex(function (x) { return x > 1; }) === 1);
check("find-last", [1, 2, 3].findLast(function (x) { return x < 3; }) === 2);
check("find-last-index", [1, 2, 3].findLastIndex(function (x) { return x < 3; }) === 1);
check("some", [1, 2].some(function (x) { return x === 2; }));
check("every", [1, 2].every(function (x) { return x > 0; }));
check("reduce", [1, 2, 3].reduce(function (t, x) { return t + x; }, 0) === 6);
check("reduce-right", ["a", "b"].reduceRight(function (t, x) { return t + x; }, "") === "ba");

let visited = 0;
[1, 2, 3].forEach(function (x) { visited = visited + x; });
check("for-each", visited === 6);

check("reverse", [1, 2, 3].reverse()[0] === 3);
check("fill", [0, 0].fill(7)[1] === 7);
check("flat", [1, [2, [3]]].flat().length === 3);
check("flat-depth", [1, [2, [3]]].flat(2).length === 3);
check("flat-map", [1, 2].flatMap(function (x) { return [x, x]; }).length === 4);
check("copy-within", [1, 2, 3, 4].copyWithin(0, 2)[0] === 3);

let spliced = [1, 2, 3, 4];
let removed = spliced.splice(1, 2);
check("splice-removed", removed.length === 2 && removed[0] === 2);
check("splice-left", spliced.length === 2 && spliced[1] === 4);

// The default comparison is by STRING, not numeric. An implementation that
// compared numbers passes every test written with single digits.
check("sort-default", [10, 9, 1].sort().join(",") === "1,10,9");
check("sort-comparator", [10, 9, 1].sort(function (x, y) { return x - y; }).join(",") === "1,9,10");
check("sort-stable", [1, 2].sort(function () { return 0; }).join(",") === "1,2");

// The ES2023 trio does not mutate.
let original = [3, 1, 2];
check("to-sorted", original.toSorted(function (x, y) { return x - y; })[0] === 1);
check("to-sorted-pure", original[0] === 3);
check("to-reversed", original.toReversed()[0] === 2);
check("with", original.with(0, 9)[0] === 9 && original[0] === 3);

check("is-array", Array.isArray([]) && !Array.isArray({}));
check("of", Array.of(1, 2).length === 2);
check("from-array", Array.from([1, 2]).length === 2);
check("from-string", Array.from("ab").length === 2);
check("keys", a.keys().length === 3);
check("values", a.values()[0] === 1);
check("entries", a.entries()[0][1] === 1);

// `for-in` over an array yields string keys and must not visit `length`.
let keys = 0;
for (let k in [1, 2, 3]) { keys = keys + 1; }
check("for-in-count", keys === 3);

let total = 0;
for (let v of [1, 2, 3]) { total = total + v; }
check("for-of", total === 6);

check("spread", [0, ...[1, 2], 3].length === 4);
check("instance-of", [] instanceof Array);

return failed;
