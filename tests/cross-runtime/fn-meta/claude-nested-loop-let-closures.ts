const closures: Array<() => number> = [];
for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 2; j++) {
    closures.push(() => i * 10 + j);
  }
}
const out: number[] = [];
for (let k = 0; k < closures.length; k++) {
  out.push(closures[k]());
}
console.log(out.join(" "));