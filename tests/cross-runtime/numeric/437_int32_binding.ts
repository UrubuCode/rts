// A local whose every store is a bitwise operator is carried in the machine's
// integer representation rather than as a double (`rts-codegen`'s
// `emit/int32.rs`). Everything below is a place where the two could be told
// apart, so it is where a wrong representation would show.
//
// `-0` is first because it is the value the narrowing loses: `0 | 0` is `+0`
// and the sign has to be gone, while a binding that merely LOOKS integral must
// keep it.
let signed = -0;
console.log(`kept -0: ${Object.is(signed, -0)} ${1 / signed}`);
let cleared = -0 | 0;
console.log(`cleared -0: ${Object.is(cleared, 0)} ${1 / cleared}`);

// The edges of the range, and the wrap at each.
let low = -2147483648;
console.log(`low ${low} ${low | 0} ${low - 1} ${(low - 1) | 0}`);
let high = 2147483647;
console.log(`high ${high} ${high + 1} ${(high + 1) | 0} ${high << 1}`);

// A loop carrying the binding, which is the shape the representation exists
// for. The value has to be the same one an ordinary double would hold.
let acc = -1;
for (let i = 0; i < 10; i++) acc = (acc << 3) ^ i;
console.log(`loop ${acc}`);

let mask = 0;
for (let i = 0; i < 40; i++) mask = mask | (1 << i);
console.log(`mask ${mask}`);

// `>>>` is deliberately NOT an int32 operation: its result is ToUint32, so a
// binding assigned from one stays a double. If it were admitted, this line
// would print a negative number.
let unsigned = 0;
for (let i = 0; i < 3; i++) unsigned = -1 >>> i;
console.log(`ushift ${unsigned} ${(-1 >>> 0) > 2147483647}`);

// Leaving the integer domain: the binding is read where a number is wanted.
let bits = 0;
for (let i = 0; i < 8; i++) bits = bits ^ (i << 2);
console.log(`out ${bits + 0.5} ${bits / 2} ${String(bits)} ${typeof bits}`);
console.log(`arr ${[bits, bits | 1].join(",")} ${JSON.stringify({ bits })}`);

// A binding that starts integral and stops being: every store decides, not the
// first one, so this must be a plain double throughout.
let mixed = 5;
for (let i = 0; i < 3; i++) mixed = i === 1 ? mixed + 0.5 : mixed | 0;
console.log(`mixed ${mixed}`);
