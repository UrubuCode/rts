// Cross-runtime: advanced string methods.
const s = "  cafe  ";
console.log("padStart=" + "7".padStart(4, "0"));
console.log("padEnd=" + "7".padEnd(4, "."));
console.log("repeat=" + "xo".repeat(3));
console.log("normalize=" + "Cafe".normalize());
console.log("trimStart=" + s.trimStart() + "|");
console.log("trimEnd=|" + s.trimEnd());
console.log("includes=" + "alphabet".includes("ha", 2));
console.log("starts=" + "alphabet".startsWith("pha", 2));
console.log("sliceNeg=" + "alphabet".slice(-3));
console.log("splitLimit=" + "a-b-c-d".split("-", 3).join(","));
