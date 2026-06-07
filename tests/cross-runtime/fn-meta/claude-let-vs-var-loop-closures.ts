const fns: Array<() => number> = [];
for (let i = 0; i < 3; i++) {
  fns.push(() => i);
}
console.log(fns[0]());
console.log(fns[1]());
console.log(fns[2]());

const vfns: Array<() => number> = [];
for (var j = 0; j < 3; j++) {
  vfns.push(() => j);
}
console.log(vfns[0]());
console.log(vfns[1]());
console.log(vfns[2]());