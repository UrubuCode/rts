// Cross-runtime: sticky matching succeeds only at the current cursor.
const re = /\w+/y;
const s = "one two";
re.lastIndex = 0;
console.log(re.exec(s)?.[0], re.lastIndex);
re.lastIndex = 3;
console.log(re.exec(s), re.lastIndex);
re.lastIndex = 4;
console.log(re.exec(s)?.[0], re.lastIndex);

