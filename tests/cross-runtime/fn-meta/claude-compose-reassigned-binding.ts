function compose(f: (x: number) => number, g: (x: number) => number) {
  return function (x: number): number {
    return f(g(x));
  };
}
const inc = (n: number) => n + 1;
const dbl = (n: number) => n * 2;
const incThenDbl = compose(dbl, inc);
const dblThenInc = compose(inc, dbl);
console.log(incThenDbl(5));
console.log(dblThenInc(5));
const triple = compose(compose(inc, inc), inc);
console.log(triple(0));

let pipe = (x: number) => x;
const ops = [inc, dbl, inc];
for (let i = 0; i < ops.length; i++) {
  const prev = pipe;
  const op = ops[i];
  pipe = (x: number) => op(prev(x));
}
console.log(pipe(1));