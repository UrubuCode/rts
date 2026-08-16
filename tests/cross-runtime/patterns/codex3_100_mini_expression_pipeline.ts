// Cross-runtime: a miniature expression pipeline combines regex, token iteration, closures, Map, and reduction.
const operators = new Map<string, (a: number, b: number) => number>([
  ["+", (a, b) => a + b],
  ["*", (a, b) => a * b],
]);
function evaluate(input: string) {
  const tokens = [...input.matchAll(/\d+|[+*]/g)].map((m) => m[0]);
  let value = Number(tokens[0]);
  const trace: string[] = [String(value)];
  for (let i = 1; i < tokens.length; i += 2) {
    const op = operators.get(tokens[i])!;
    value = op(value, Number(tokens[i + 1]));
    trace.push(tokens[i] + tokens[i + 1] + "=" + value);
  }
  return value + "|" + trace.join(",");
}
console.log(evaluate("2 + 3 * 4 + 5"));
console.log(evaluate("10 * 2 + 7"));

