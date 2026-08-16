// Cross-runtime: replace callbacks receive match, captures, offset, and input.
const seen: string[] = [];
const out = "a1 b22".replace(/([a-z])(\d+)/g, (match, letter, digits, offset, input) => {
  seen.push([match, letter, digits, offset, input.length].join(":"));
  return letter.toUpperCase() + Number(digits) * 2;
});
console.log(out);
console.log(seen.join("|"));

