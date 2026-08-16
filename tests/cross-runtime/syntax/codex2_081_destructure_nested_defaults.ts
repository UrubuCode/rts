// Cross-runtime: nested destructuring defaults apply only to undefined.
const source: any = { a: { x: 0 }, b: undefined, c: null };
const { a: { x = 9, y = 8 }, b = 7, c = 6 } = source;
console.log(x, y, b, c);

