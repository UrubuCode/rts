function makeCounter(): () => number {
  let count = 0;
  return function (): number {
    count = count + 1;
    return count;
  };
}
const c = makeCounter();
console.log(c());
console.log(c());
console.log(c());
const d = makeCounter();
console.log(d());
console.log(c());