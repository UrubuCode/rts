// Cross-runtime: arguments object has callee in sloppy functions.
const run = Function(`
  function f() {
    console.log(arguments.length);
    console.log(typeof arguments.callee);
    console.log(arguments.callee === f);
  }
  f(1, 2, 3);
`);
run();
