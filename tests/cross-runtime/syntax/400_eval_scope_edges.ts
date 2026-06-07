// Cross-runtime: direct vs indirect eval scope behavior.
const x = "global";

function probe() {
  const x = "local";
  const direct = eval("x");
  const indirect = (0, eval)("typeof x === 'undefined' ? 'missing' : x");
  return direct + ":" + indirect;
}

console.log(probe());
console.log(eval("1 + 2 * 3"));
