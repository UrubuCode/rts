// Cross-runtime: Uint8ClampedArray uses clamp plus ties-to-even rounding.
const values = new Uint8ClampedArray([-1, 0.5, 1.5, 2.5, 3.5, 254.5, 255, 300, NaN]);
console.log(values.join(","));
values[0] = Infinity;
values[1] = -Infinity;
console.log(values[0], values[1]);

