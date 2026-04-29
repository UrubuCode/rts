import { describe, test, expect } from "rts:test";

let __rtsCapturedOutput: string = "";
function print(value: string): void {
  __rtsCapturedOutput += value + "\n";
}

// Basic tagged template
function tag(strings: TemplateStringsArray, ...values: any[]): string {
  let result = "";
  strings.forEach((str, i) => {
    result += str;
    if (i < values.length) result += String(values[i]).toUpperCase();
  });
  return result;
}

print(tag`Hello ${"world"} and ${"foo"}`);
print(tag`Value: ${42}`);

// html escape tag
function html(strings: TemplateStringsArray, ...values: any[]): string {
  return strings.reduce((acc, str, i) => {
    const val = i < values.length
      ? String(values[i]).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
      : "";
    return acc + str + val;
  }, "");
}

const user = "<script>alert('xss')</script>";
print(html`<div>Hello ${user}</div>`);

// raw strings
function raw(strings: TemplateStringsArray): string {
  return strings.raw[0];
}
print(raw`line1\nline2`);

describe("tagged_template", () => {
  test("basic", () => expect(__rtsCapturedOutput).toBe(
    "Hello WORLD and FOO\nValue: 42\n<div>Hello &lt;script&gt;alert('xss')&lt;/script&gt;</div>\nline1\\nline2\n"
  ));
});
