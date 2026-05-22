// Cross-runtime: tagged template literals.
function highlight(strings: TemplateStringsArray, ...values: any[]) {
  return strings.reduce((acc, str, i) => acc + str + (values[i] !== undefined ? "[" + values[i] + "]" : ""), "");
}

const name = "world";
const n = 42;
console.log(highlight`hello ${name} the number is ${n}`);
console.log(highlight`no interpolation`);
console.log(highlight`${1}${2}${3}`);

// String.raw
console.log(String.raw`line1\nline2`);   // raw — \n not escaped
console.log(String.raw`tab\there`);

// tag that returns array
function parts(strings: TemplateStringsArray, ...vals: any[]) {
  return [...strings];
}
const result = parts`a${1}b${2}c`;
console.log(result.join("|"));

// tag with raw access
function rawVsCooked(strings: TemplateStringsArray) {
  console.log(strings[0]);
  console.log(strings.raw[0]);
}
rawVsCooked`line1\nline2`;
