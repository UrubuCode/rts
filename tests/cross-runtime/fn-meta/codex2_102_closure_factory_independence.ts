// Cross-runtime: separate factory calls allocate independent captured cells.
function counter(start: number) {
  let n = start;
  return () => ++n;
}
const a = counter(0);
const b = counter(10);
console.log(a(), a(), b(), a(), b());

