// Cross-runtime: sticky Unicode matching advances by full code points at valid boundaries.
const s = "A😀B";
const re = /./uy;
for (const index of [0, 1, 3, 4]) {
  re.lastIndex = index;
  const match = re.exec(s);
  console.log(index, match?.[0], match?.index, re.lastIndex);
}
