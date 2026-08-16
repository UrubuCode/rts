// Cross-runtime: an optional call skips argument evaluation when nullish.
let calls = 0;
const arg = () => { calls++; return 5; };
const absent: any = null;
const present: any = (x: number) => x * 2;
console.log(absent?.(arg()), calls);
console.log(present?.(arg()), calls);

