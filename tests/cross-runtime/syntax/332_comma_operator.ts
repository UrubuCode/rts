// Cross-runtime: comma operator (evaluates both, returns last).
const a = (1, 2, 3);
console.log(a);

let x = 0;
const b = (x++, x++, x);
console.log(b);
console.log(x);

// in for loop — multiple expressions
for (let i = 0, j = 10; i < 3; i++, j -= 3) {
  console.log(i + ":" + j);
}

// comma in return
function last() { return (console.log("side"), 42); }
const r = last();
console.log(r);
