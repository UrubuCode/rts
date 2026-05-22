// Cross-runtime compatibility: tagged templates and String.raw.
function tag(strings: TemplateStringsArray, ...values: unknown[]): string {
  return strings.raw.join("|") + "::" + values.join(",");
}

const value = 42;
console.log("raw=" + String.raw`line1\n${value}\tend`);
console.log("tag=" + tag`a\n${value}b\t${"x"}`);
