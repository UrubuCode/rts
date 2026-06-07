// Cross-runtime: RegExp d flag indices with named groups.
const re: any = /(?<word>a+)(b)/d;
const m = re.exec("xxaaabyy");
console.log(m[0] + ":" + m.index);
console.log(JSON.stringify(m.indices[0]));
console.log(JSON.stringify(m.indices.groups.word));
console.log(m.groups.word);
