// Cross-runtime: sloppy arguments aliasing vs rest parameters.
function sloppy(a: any, b: any) {
  arguments[0] = "A";
  b = "B";
  return a + ":" + arguments[1];
}

function rest(a: any, ...xs: any[]) {
  xs[0] = "X";
  a = "Y";
  return a + ":" + xs[0] + ":" + arguments[0];
}

console.log(sloppy("a", "b"));
console.log(rest("a", "b"));
