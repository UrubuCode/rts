// Cross-runtime: global replace with zero-length matches must advance.
const hits: string[] = [];
const out = "ab".replace(/(?=.)/g, (m, offset) => {
  hits.push(offset + ":" + m.length);
  return "|";
});

console.log(out);
console.log(hits.join(","));
