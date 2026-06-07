// Cross-runtime: direct eval can mutate captured outer bindings.
function make() {
  let x = 1;
  return {
    inc() {
      eval("x += 2");
      return x;
    },
    read() {
      return x;
    }
  };
}

const m = make();
console.log(m.inc());
console.log(m.inc());
console.log(m.read());
