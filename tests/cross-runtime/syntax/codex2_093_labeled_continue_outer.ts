// Cross-runtime: labeled continue advances the selected outer loop.
const pairs: string[] = [];
outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (j === 1) continue outer;
    pairs.push(i + ":" + j);
  }
}
console.log(pairs.join("|"));

