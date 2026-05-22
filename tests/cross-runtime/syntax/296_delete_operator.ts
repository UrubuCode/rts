// Cross-runtime: delete operator on property and array index.
const obj: any = { a: 1, b: 2 };
const arr = [1, 2, 3];
console.log("obj=" + delete obj.a + ":" + JSON.stringify(obj));
console.log("arr=" + delete arr[1] + ":" + arr.length + ":" + arr.join("|"));
