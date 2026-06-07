function makeAccumulators(): Array<(x: number) => number> {
  const list: Array<(x: number) => number> = [];
  for (let i = 0; i < 3; i++) {
    let total = 0;
    list.push(function (x: number): number {
      total += x * (i + 1);
      return total;
    });
  }
  return list;
}
const accs = makeAccumulators();
console.log(accs[0](1));
console.log(accs[0](1));
console.log(accs[1](1));
console.log(accs[2](1));
console.log(accs[2](1));