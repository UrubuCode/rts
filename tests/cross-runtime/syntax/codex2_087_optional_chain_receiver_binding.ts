// Cross-runtime: optional method calls preserve their receiver.
const o: any = {
  value: 4,
  method(mult: number) { return this.value * mult; },
};
console.log(o.method?.(3));
console.log(o.missing?.(3));
console.log(o?.method(2));

