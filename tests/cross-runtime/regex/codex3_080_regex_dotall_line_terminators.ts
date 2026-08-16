// Cross-runtime: dotAll spans every ECMAScript line terminator while plain dot does not.
const separators = ["\n", "\r", "\u2028", "\u2029"];
for (const separator of separators) {
  const input = "a" + separator + "b";
  console.log(/a.b/.test(input), /a.b/s.test(input));
}
console.log(new RegExp(".", "s").dotAll);

