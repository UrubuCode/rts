// Cross-runtime: tagged templates expose parallel cooked and raw segments.
function tag(parts: TemplateStringsArray, ...values: any[]) {
  return parts.map((p, i) => p + "/" + parts.raw[i] + "/" + (values[i] ?? "")).join("|");
}
console.log(tag`a\nb${7}c\td`);

