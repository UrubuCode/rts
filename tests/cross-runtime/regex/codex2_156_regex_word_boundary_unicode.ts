// Cross-runtime: word boundaries follow the engine's ASCII word-character rules.
const s = "cat_scatter 42x élan";
console.log(JSON.stringify(s.match(/\b\w+\b/g)));
console.log(/\bcat\b/.test(s), /\bscatter\b/.test(s));

