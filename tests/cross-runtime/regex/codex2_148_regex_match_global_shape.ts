// Cross-runtime: match with global mode returns matches without capture metadata.
const input = "a1 b22 c333";
const global = input.match(/[a-z](\d+)/g);
const single = input.match(/[a-z](\d+)/);
console.log(JSON.stringify(global));
console.log(single![0], single![1], single!.index);

