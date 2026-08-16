// Cross-runtime: numbered and named backreferences require repeated text.
console.log(/^(\w+)\s+\1$/.test("echo echo"));
console.log(/^(\w+)\s+\1$/.test("echo other"));
console.log(/^(?<x>ab)-\k<x>$/.test("ab-ab"));

