// Cross-runtime: Reflect.construct runs one constructor with another constructor's prototype.
function Source(this: any, value: number) { this.value = value; }
function NewTarget(this: any) {}
(NewTarget as any).prototype.kind = "custom";
const value: any = Reflect.construct(Source, [7], NewTarget);
console.log(value.value, value.kind);
console.log(value instanceof Source, value instanceof NewTarget);
console.log(Object.getPrototypeOf(value) === NewTarget.prototype);

