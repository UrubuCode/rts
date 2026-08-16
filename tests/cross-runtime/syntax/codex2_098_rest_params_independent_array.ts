// Cross-runtime: each rest parameter is an independent mutable array.
function collect(head: number, ...tail: number[]) {
  tail.push(head);
  return tail;
}
const source = [1, 2, 3];
const out = collect(...source);
console.log(source.join(","), out.join(","));
console.log(source === out);

