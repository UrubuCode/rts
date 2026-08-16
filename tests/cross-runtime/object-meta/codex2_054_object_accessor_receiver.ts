// Cross-runtime: inherited accessors receive the actual lookup receiver.
const proto = {
  get doubled() { return (this as any).value * 2; },
  set doubled(v) { (this as any).value = v / 2; },
};
const o: any = Object.create(proto);
o.value = 5;
console.log(o.doubled);
o.doubled = 18;
console.log(o.value, Object.hasOwn(o, "doubled"));

