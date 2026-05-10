// Cross-runtime: tagged templates and raw strings.
function tag(strings: TemplateStringsArray, ...values: unknown[]) {
  return strings.raw.join("|") + "::" + values.join(",");
}
console.log("raw=" + String.raw`a\n${1}b\t`);
console.log("tag=" + tag`x\n${2}y\t${"z"}`);
