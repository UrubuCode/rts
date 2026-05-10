// Cross-runtime: arguments object shape and iterability.
function f() {
  return [
    arguments.length,
    arguments[0],
    Array.from(arguments as any).join(","),
    typeof (arguments as any)[Symbol.iterator],
  ].join("|");
}
console.log("args=" + f(1, 2, 3));
