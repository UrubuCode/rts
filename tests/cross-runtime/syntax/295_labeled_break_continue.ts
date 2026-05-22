// Cross-runtime: labeled break and continue.
let out: string[] = [];
outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (i === 1 && j === 0) continue outer;
    if (i === 2 && j === 1) break outer;
    out.push(i + ":" + j);
  }
}
console.log("out=" + out.join("|"));
