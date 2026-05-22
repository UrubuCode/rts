// Cross-runtime: labeled break/continue in nested loops.
outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (j === 1) continue outer;
    console.log(i + "," + j);
  }
}

// labeled break
search: for (let i = 0; i < 5; i++) {
  for (let j = 0; j < 5; j++) {
    if (i * j > 6) {
      console.log("found at " + i + "," + j);
      break search;
    }
  }
}

// labeled block (rare — label on block, not loop)
block1: {
  console.log("before break");
  break block1;
  console.log("unreachable");
}
console.log("after block");
