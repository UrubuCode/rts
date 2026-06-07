function isEven(n: number): boolean {
  if (n === 0) return true;
  return isOdd(n - 1);
}
function isOdd(n: number): boolean {
  if (n === 0) return false;
  return isEven(n - 1);
}
console.log(isEven(10));
console.log(isOdd(10));
console.log(isEven(7));
console.log(isOdd(7));
console.log(isEven(0));
console.log(isOdd(1));