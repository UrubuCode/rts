// Cross-runtime: arguments is iterable in modern runtimes.
function collect() {
  return [...arguments].map((x) => String(x).toUpperCase()).join(",");
}
console.log(collect("a", 2, true));
console.log(collect());

