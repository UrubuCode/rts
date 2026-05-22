// Cross-runtime: optional catch binding (no error variable)
let result = "none";

try {
  throw new Error("test");
} catch {
  result = "caught";
}

console.log("result=" + result);

// With binding
try {
  throw new Error("msg");
} catch (e) {
  result = "with:" + (e as Error).message;
}

console.log("result2=" + result);
