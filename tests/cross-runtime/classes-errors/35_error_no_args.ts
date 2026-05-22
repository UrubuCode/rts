// Cross-runtime: built-in errors without explicit message.
try {
  throw new Error();
} catch (e: any) {
  console.log("err=" + e.name + ":[" + e.message + "]");
}

try {
  throw new TypeError();
} catch (e: any) {
  console.log("type=" + e.name + ":[" + e.message + "]");
}
