// Cross-runtime: Function.name and length
function foo(a: number, b: number) {
  return a + b;
}

console.log("name=" + foo.name);
console.log("length=" + foo.length);

const bar = function(x: number) {
  return x * 2;
};
console.log("anon_name=" + bar.name);
console.log("anon_length=" + bar.length);

const arrow = (a: number, b: number, c: number) => a + b + c;
console.log("arrow_name=" + arrow.name);
console.log("arrow_length=" + arrow.length);

const noArgs = () => 42;
console.log("noargs_length=" + noArgs.length);
