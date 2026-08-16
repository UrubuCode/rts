// Cross-runtime: nested closures resolve the nearest shadowed binding.
const x = "outer";
function make(x: string) {
  return function middle() {
    const y = x + "-middle";
    return function inner(x: string) {
      return [x, y].join("|");
    };
  };
}
console.log(make("param")()("inner"));
console.log(x);

