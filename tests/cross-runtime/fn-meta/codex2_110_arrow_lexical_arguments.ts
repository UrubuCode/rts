// Cross-runtime: an arrow reads the enclosing function's arguments.
export {};
function outer(a: any, b: any) {
  const read = () => [arguments[0], arguments[1], arguments.length].join("|");
  a = "changed";
  return read();
}
console.log(outer("x", "y"));
