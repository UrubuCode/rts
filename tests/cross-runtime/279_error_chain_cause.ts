// Cross-runtime: chained error.cause traversal.
const leaf = new RangeError("leaf");
const mid = new Error("mid", { cause: leaf as any });
const top = new TypeError("top", { cause: mid as any });
console.log("top=" + top.message);
console.log("mid=" + (top as any).cause.message);
console.log("leaf=" + (top as any).cause.cause.message);
