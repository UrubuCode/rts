// Cross-runtime: named-capture replace callbacks receive groups as their final argument.
const seen: string[] = [];
const out = "x=12;y=3".replace(/(?<key>[a-z])=(?<value>\d+)/g, (...args: any[]) => {
  const groups = args[args.length - 1];
  const offset = args[args.length - 3];
  seen.push(groups.key + ":" + groups.value + ":" + offset);
  return groups.key.toUpperCase() + Number(groups.value) * 2;
});
console.log(out);
console.log(seen.join("|"));

