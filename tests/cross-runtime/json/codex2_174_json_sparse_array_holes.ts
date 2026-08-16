// Cross-runtime: sparse array holes serialize as null entries.
const a: any[] = [];
a.length = 5;
a[1] = "x";
a[4] = undefined;
console.log(JSON.stringify(a));
console.log(Object.keys(a).join(","), a.length);

