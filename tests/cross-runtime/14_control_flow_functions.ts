// Cross-runtime: plain functions and deterministic control flow.
function sumRange(limit: number): number {
  let total = 0;
  for (let i = 0; i <= limit; i++) {
    total += i;
  }
  return total;
}

function classify(n: number): string {
  if (n < 0) return "neg";
  if (n === 0) return "zero";
  return "pos";
}

function joinParts(a: string, b: string, c: string = "end"): string {
  return a + ":" + b + ":" + c;
}

console.log("sum3=" + sumRange(3));
console.log("sum5=" + sumRange(5));
console.log("class_a=" + classify(-1));
console.log("class_b=" + classify(0));
console.log("class_c=" + classify(2));
console.log("join_a=" + joinParts("x", "y"));
console.log("join_b=" + joinParts("x", "y", "z"));
