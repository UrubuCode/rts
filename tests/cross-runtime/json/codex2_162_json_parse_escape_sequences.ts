// Cross-runtime: JSON string escapes decode control and Unicode sequences.
const value = JSON.parse('"line\\nquote\\\"slash\\\\tab\\tA\\u0041"');
console.log(JSON.stringify(value));
console.log(value.length);

