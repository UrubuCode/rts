// Cross-runtime: void operator always yields undefined.
console.log("a=" + String(void 0));
console.log("b=" + String(void (1 + 2)));
