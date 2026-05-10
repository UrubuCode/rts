// Cross-runtime: logical operators + truthiness com strings.
// Esperado: RTS = Bun = Node em todos os outputs.
console.log("'' || 'fb'=" + ("" || "fb"));
console.log("'a' || 'fb'=" + ("a" || "fb"));
console.log("'' && 'x'=" + ("" && "x"));
console.log("'a' && 'x'=" + ("a" && "x"));
console.log("'' ? 'y' : 'n'=" + ("" ? "y" : "n"));
console.log("'a' ? 'y' : 'n'=" + ("a" ? "y" : "n"));
console.log("0 ? 'y' : 'n'=" + (0 ? "y" : "n"));
console.log("1 ? 'y' : 'n'=" + (1 ? "y" : "n"));
console.log("null ? 'y' : 'n'=" + (null ? "y" : "n"));
console.log("'   ' ? 'y' : 'n'=" + ("   " ? "y" : "n"));
console.log("chain=" + ("" || "" || "fb" || "x"));
console.log("prec='a' || 'b' && 'c'=" + ("a" || "b" && "c"));
console.log("prec='' || 'x' && 'y'=" + ("" || "x" && "y"));
