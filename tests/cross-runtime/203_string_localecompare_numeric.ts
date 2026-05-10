// Cross-runtime: String.localeCompare basic behavior
const strings = ["10", "2", "1", "20"];
const sorted = strings.sort((a, b) => a.localeCompare(b));
console.log("alpha=" + sorted.join(","));

// Numeric comparison would need options, which may vary
const simple = ["a", "b", "c"];
const simpleSorted = simple.sort((a, b) => a.localeCompare(b));
console.log("simple=" + simpleSorted.join(","));

console.log("compare=" + "a".localeCompare("b"));
console.log("equal=" + "a".localeCompare("a"));
console.log("greater=" + "b".localeCompare("a"));
