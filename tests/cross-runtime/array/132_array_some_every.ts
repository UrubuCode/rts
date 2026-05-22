// Cross-runtime: Array.some and every with edge cases
const arr = [2, 4, 6, 8];
console.log("all_even=" + arr.every(x => x % 2 === 0));
console.log("some_gt5=" + arr.some(x => x > 5));
console.log("some_gt10=" + arr.some(x => x > 10));

const mixed = [1, 2, 3, 4];
console.log("mixed_all_even=" + mixed.every(x => x % 2 === 0));
console.log("mixed_some_even=" + mixed.some(x => x % 2 === 0));

const empty: number[] = [];
console.log("empty_every=" + empty.every(x => x > 0));
console.log("empty_some=" + empty.some(x => x > 0));

// Early termination
let everyCount = 0;
[1, 2, 3, 4].every(x => { everyCount++; return x < 3; });
console.log("every_count=" + everyCount);

let someCount = 0;
[1, 2, 3, 4].some(x => { someCount++; return x > 2; });
console.log("some_count=" + someCount);
