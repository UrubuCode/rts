// Cross-runtime: replace callback args with named captures.
const s = "red:10 blue:20";
const out = s.replace(/(?<name>[a-z]+):(?<value>\d+)/g, (...args: any[]) => {
  const groups = args[args.length - 1];
  const offset = args[args.length - 3];
  return groups.name.toUpperCase() + "@" + offset + "=" + (Number(groups.value) + 1);
});

console.log(out);
console.log("aba".replace(/(?=(a))/g, "[$1]"));
