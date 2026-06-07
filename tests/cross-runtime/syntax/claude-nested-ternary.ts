function grade(n: number): string {
  return n >= 90 ? "A" : n >= 80 ? "B" : n >= 70 ? "C" : "F";
}
console.log(grade(95));
console.log(grade(85));
console.log(grade(72));
console.log(grade(50));
let a = 1, b = 0, c = 1;
let r = a ? b ? "ab" : c ? "ac" : "a" : "none";
console.log(r);
let x = true ? false ? 1 : 2 : 3;
console.log(x);
let y = false ? 1 : true ? 2 : 3;
console.log(y);
let chain = 0 ? "x" : "" ? "y" : null ? "z" : "fallback";
console.log(chain);