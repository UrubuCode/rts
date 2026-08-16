// Cross-runtime: switch fallthrough works with default between cases.
function classify(x: number) {
  const out: string[] = [];
  switch (x) {
    case 1: out.push("one");
    default: out.push("default");
    case 2: out.push("two"); break;
    case 3: out.push("three");
  }
  return out.join(",");
}
console.log([1, 2, 9, 3].map(classify).join("|"));

