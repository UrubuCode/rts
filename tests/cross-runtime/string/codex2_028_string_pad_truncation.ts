// Cross-runtime: padding repeats and truncates the fill string to exact width.
console.log("7".padStart(6, "abc"));
console.log("xy".padEnd(7, "01"));
console.log("already".padStart(3, "z"), "same".padEnd(4, ""));

