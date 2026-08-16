// Cross-runtime: closures created in defaults retain the parameter environment.
let x = "outer";
function make(x = "param", read = () => x) {
  var x = "body";
  return [read(), x];
}
console.log(make().join("|"));
console.log(x);

