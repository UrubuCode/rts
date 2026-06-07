function f(): void {
  console.log(x);
  var x = 5;
  console.log(x);
}
f();

function g(): void {
  console.log(typeof hoisted);
  console.log(hoisted());
  function hoisted(): number {
    return 42;
  }
  console.log(hoisted());
}
g();