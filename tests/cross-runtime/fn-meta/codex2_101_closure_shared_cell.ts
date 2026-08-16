// Cross-runtime: sibling closures share one mutable lexical cell.
function pair() {
  let value = 0;
  return {
    add(n: number) { value += n; return value; },
    read() { return value; },
  };
}
const p = pair();
console.log(p.add(2), p.read(), p.add(5), p.read());

